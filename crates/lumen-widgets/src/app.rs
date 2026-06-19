//! The application and its headless runtime (02 §8).
//!
//! `Headless::pump` runs one turn: drain input → rebuild the element tree →
//! lay out → paint to the CPU renderer → build the semantic tree. It integrates
//! lumen-core (tree/state/events/semantics), lumen-layout, lumen-render, and
//! lumen-text. Interactive state (focus/hover) is keyed by [`StableId`] so it
//! survives the from-scratch rebuild.

use crate::element::{BuildCx, Element, Handler, NodeContent};
use kurbo::{Point, Rect, Size};
use lumen_core::events::{Event, InputQueue, Key, NamedKey, PointerState};
use lumen_core::semantics::{
    Action, Role, SemanticsDoc, SemanticsNode, State as SemState, WindowInfo,
};
use lumen_core::state::Runtime;
use lumen_core::tree::{NodeFlags, Tree};
use lumen_core::{Color, NodeIndex, StableId};
use lumen_layout::{Dim, LayoutNode, LayoutStyle, LayoutTree};
use lumen_render::{cpu, Brush, CornerRadii, DisplayList, DrawCmd, RgbaImage};
use lumen_text::TextEngine;
use std::collections::HashMap;

/// Statistics for one rendered frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameStats {
    /// Number of live nodes after the rebuild.
    pub node_count: usize,
    /// Whether a frame was painted.
    pub painted: bool,
}

/// An application: a root build closure plus an optional stylesheet.
pub struct App {
    root: Box<dyn Fn(&mut BuildCx) -> Element>,
    #[allow(dead_code)]
    stylesheet: Option<String>,
}

impl App {
    /// Create an app from its root build closure (02 §8).
    pub fn new(root: impl Fn(&mut BuildCx) -> Element + 'static) -> App {
        App {
            root: Box::new(root),
            stylesheet: None,
        }
    }

    /// Attach a stylesheet (parsed in M1; stored for now).
    pub fn stylesheet(mut self, lss: &str) -> App {
        self.stylesheet = Some(lss.to_string());
        self
    }

    /// Run headless on the CPU renderer at `size` (no OS dependencies).
    pub fn run_headless(self, size: Size) -> Headless {
        self.boot(size, None).0
    }

    /// Run headless, restoring a prior [`AppSnapshot`] (tier-3 restart,
    /// ADR-011). Returns the instance plus any `W0002` drop diagnostics raised
    /// when a snapshot value no longer has a matching signal.
    pub fn run_headless_restored(
        self,
        size: Size,
        snap: AppSnapshot,
    ) -> (Headless, Vec<lumen_core::Diagnostic>) {
        self.boot(size, Some(snap))
    }

    fn boot(
        self,
        size: Size,
        restore: Option<AppSnapshot>,
    ) -> (Headless, Vec<lumen_core::Diagnostic>) {
        // Focus is host state (not in the reactive store), so it is carried on
        // the snapshot and re-applied directly.
        let focused = restore.as_ref().and_then(|s| s.focused.clone());
        let mut h = Headless {
            root: self.root,
            rt: Runtime::new(),
            size,
            scale: 1.0,
            clock_ms: 0.0,
            renderer: Box::new(lumen_render::CpuRenderer),
            text: TextEngine::new(),
            text_cache: HashMap::new(),
            shadow_cache: HashMap::new(),
            tree: Tree::new(),
            meta: HashMap::new(),
            frame: RgbaImage::new(size.width as u32, size.height as u32),
            sem_root: None,
            build_panic: None,
            focused_id: focused,
            hovered_id: None,
            pressed: None,
            input: InputQueue::new(),
            pointer: PointerState::new(),
            requests: crate::element::FrameRequests::default(),
            app_sheet: self.stylesheet.as_deref().and_then(parse_sheet),
            theme: lumen_style::ThemeKind::Light,
            node_style: HashMap::new(),
            node_computed: HashMap::new(),
            clipboard: String::new(),
            menu: crate::system::MenuModel::default(),
            invoked_menu: Vec::new(),
            system_requests: Vec::new(),
            windows: Vec::new(),
            rtl: false,
        };
        let diags = if let Some(s) = restore {
            // Stage the snapshot *before* the first build so each signal adopts
            // its restored value as it is (re-)created (Checkpoint protocol).
            h.rt.load_pending(s.state);
            h.rebuild();
            h.rt.finish_restore()
        } else {
            h.rebuild();
            Vec::new()
        };
        (h, diags)
    }
}

/// A tier-3 snapshot of a running app: the reactive store (every signal —
/// including scroll offsets) plus focus. Serializable, so it can be written
/// before a process restart and restored afterwards (ADR-011).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSnapshot {
    state: lumen_core::state::StateSnapshot,
    focused: Option<StableId>,
}

/// Parse a stylesheet, returning it only if error-free.
fn parse_sheet(src: &str) -> Option<lumen_style::Stylesheet> {
    let (sheet, diags) = lumen_style::parse("app.lss", src);
    (!lumen_style::has_errors(&diags)).then_some(sheet)
}

/// The result of a tier-1 hot reload (03 §3 reload event).
#[derive(Clone, Debug)]
pub enum ReloadResult {
    /// The stylesheet applied; styles changed live.
    Ok,
    /// The edit was rejected; the previous stylesheet stays live.
    Failed(Vec<lumen_core::Diagnostic>),
}

struct NodeMeta {
    id: Option<StableId>,
    role: Role,
    label: String,
    value: Option<String>,
    classes: Vec<String>,
    actions: Vec<Action>,
    states: Vec<SemState>,
    scroll: Option<lumen_core::semantics::ScrollInfo>,
    focusable: bool,
    elide: bool,
    on_click: Option<Handler>,
    on_wheel: Option<crate::element::WheelHandler>,
    on_drag: Option<crate::element::DragHandler>,
    on_drop: Option<crate::element::DropHandler>,
    on_text: Option<crate::element::TextHandler>,
    background: Option<Color>,
    corner_radius: f64,
    shadow: Option<crate::element::Shadow>,
    content: NodeContent,
    /// Left/top padding in px — own-text is painted at the padded (content-box)
    /// origin, so a button label sits inside its padding instead of jammed into
    /// the border-box corner.
    pad: (f64, f64),
}

/// The px value of a [`Dim`] (0 for non-px / auto / percent).
fn dim_px(d: Dim) -> f64 {
    match d {
        Dim::Px(v) => v as f64,
        _ => 0.0,
    }
}

/// A headless, CPU-rendered application instance (02 §8). Drives the same input
/// queue as a real shell, so tests and the agent exercise the real paths.
pub struct Headless {
    root: Box<dyn Fn(&mut BuildCx) -> Element>,
    rt: Runtime,
    /// Logical size (the coordinate space for layout, events, and the display
    /// list). The rasterized frame is this times [`Headless::scale`].
    size: Size,
    /// HiDPI scale factor: the frame is rendered at `size * scale` physical px.
    scale: f64,
    clock_ms: f64,
    /// The pluggable frame renderer (A1). Defaults to the CPU reference renderer;
    /// `set_renderer` swaps in another backend (e.g. GPU) at runtime.
    renderer: Box<dyn lumen_render::Renderer>,
    text: TextEngine,
    /// Cache of rasterized text keyed by (string, size bits, weight bits, sRGB
    /// color): static labels then cost one memcpy per frame instead of a full
    /// reshape + glyph raster. Cleared wholesale when it exceeds a cap so an
    /// animated readout (many distinct strings) can't grow it without bound.
    text_cache: HashMap<(String, u32, u32, u32), RgbaImage>,
    /// Cache of rasterized drop shadows keyed by quantized (w, h, radius, blur,
    /// spread, color). The stacked-rounded-rect penumbra is the single most
    /// expensive thing in a typical frame; since it's static for a given box it
    /// is rendered once and then blitted as one image.
    shadow_cache: HashMap<(i32, i32, i32, i32, i32, u32), RgbaImage>,
    tree: Tree,
    meta: HashMap<NodeIndex, NodeMeta>,
    frame: RgbaImage,
    sem_root: Option<SemanticsNode>,
    /// If the last build panicked, the contained diagnostic (the previous good
    /// frame is kept). Cleared on the next successful build (C2 / T7.3).
    build_panic: Option<lumen_core::Diagnostic>,
    focused_id: Option<StableId>,
    hovered_id: Option<StableId>,
    pressed: Option<NodeIndex>,
    app_sheet: Option<lumen_style::Stylesheet>,
    theme: lumen_style::ThemeKind,
    node_style: HashMap<NodeIndex, lumen_style::Style>,
    node_computed: HashMap<NodeIndex, HashMap<String, lumen_style::Computed>>,
    input: InputQueue,
    pointer: PointerState,
    // Animation/timer requests from the latest build (02 §8, time-driven UI).
    requests: crate::element::FrameRequests,
    // Desktop system integration (T5.2).
    clipboard: String,
    menu: crate::system::MenuModel,
    invoked_menu: Vec<String>,
    system_requests: Vec<crate::system::SystemRequest>,
    windows: Vec<crate::system::WindowDesc>,
    rtl: bool,
}

impl Headless {
    /// Process the input queue, then rebuild/layout/paint/semantics one turn.
    pub fn pump(&mut self) -> FrameStats {
        let mut events = Vec::new();
        while let Some(ev) = self.input.pop() {
            events.push(ev);
        }
        for ev in events {
            self.route(ev);
        }
        self.rebuild();
        FrameStats {
            node_count: self.tree.len(),
            painted: true,
        }
    }

    /// Enqueue an event (OS or synthesized — same path).
    pub fn inject(&mut self, ev: Event) {
        self.input.push(ev);
    }

    /// Resize the render surface. Updates the size used for layout *and*
    /// rasterization, then re-lays-out and repaints so hit-test bounds and the
    /// rendered frame both track the new dimensions. The desktop shell calls
    /// this on `WindowEvent::Resized`; without it, layout (hence every node's
    /// hit rectangle) stays at the old size and the old-size frame gets
    /// upscaled by the presenter (blur). No-op if the size is unchanged.
    pub fn resize(&mut self, size: Size) {
        if size != self.size {
            self.size = size;
            self.pump();
        }
    }

    /// The current surface size (logical px).
    pub fn size(&self) -> Size {
        self.size
    }

    /// The current HiDPI scale factor (physical px per logical px).
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Set the HiDPI scale factor and repaint at the new physical resolution.
    /// Layout (logical) is unaffected; only the rasterized frame's pixel size
    /// changes. The desktop shell calls this on `ScaleFactorChanged`. No-op if
    /// unchanged or non-positive.
    pub fn set_scale(&mut self, scale: f64) {
        if scale > 0.0 && scale != self.scale {
            self.scale = scale;
            self.pump();
        }
    }

    /// The most recent rendered frame.
    pub fn screenshot(&mut self) -> RgbaImage {
        self.frame.clone()
    }

    /// Capture a tier-3 [`AppSnapshot`] (reactive store + focus) for a later
    /// restart via [`App::run_headless_restored`].
    pub fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            state: self.rt.snapshot(),
            focused: self.focused_id.clone(),
        }
    }

    /// The current virtual-clock time (ms).
    pub fn now_ms(&self) -> f64 {
        self.clock_ms
    }

    /// Advance the virtual clock by `ms`.
    pub fn advance_clock(&mut self, ms: f64) {
        self.clock_ms += ms;
    }

    /// Advance the virtual clock by `dt_ms` and pump one frame. The deterministic
    /// driver for time-based UI: a test calls `advance(1000.0)` to move a clock
    /// hand exactly one second; the desktop shell calls it with the real elapsed
    /// time each frame. Equivalent to [`advance_clock`](Self::advance_clock) then
    /// [`pump`](Self::pump).
    pub fn advance(&mut self, dt_ms: f64) -> FrameStats {
        self.advance_clock(dt_ms);
        self.pump()
    }

    /// Whether the latest build requested continuous animation (via
    /// [`BuildCx::animate`](crate::BuildCx::animate)).
    pub fn is_animating(&self) -> bool {
        self.requests.continuous
    }

    /// The next virtual-clock time (ms) at which the UI wants a frame, or `None`
    /// if it is idle. `Some(t)` with `t <= now_ms()` means "animate now" (a
    /// continuous animation); a larger `t` is a one-shot wake. The host turns
    /// this into a wait/poll decision so an idle UI costs no frames.
    pub fn next_deadline(&self) -> Option<f64> {
        if self.requests.continuous {
            return Some(self.clock_ms);
        }
        self.requests
            .wakes
            .iter()
            .copied()
            .filter(|t| *t > self.clock_ms)
            .min_by(|a, b| a.total_cmp(b))
    }

    /// The semantics document as JSON (`lumen-semantics/1`, 03 §1).
    pub fn semantics_json(&self) -> serde_json::Value {
        self.semantics_doc().to_json(false)
    }

    /// Structured diagnostics for the current frame (e.g. `W0103` layout
    /// overflow). Lets an agent detect and fix layout bugs by code.
    pub fn diagnostics(&self) -> Vec<lumen_core::Diagnostic> {
        let mut diags = crate::audit::check_overflow(&self.semantics_doc().root);
        if let Some(d) = &self.build_panic {
            diags.push(d.clone());
        }
        diags
    }

    // --- desktop system integration (T5.2) ---------------------------------

    /// Read the (in-memory) clipboard text.
    pub fn clipboard_read(&self) -> String {
        self.clipboard.clone()
    }

    /// Write text to the clipboard.
    pub fn clipboard_write(&mut self, text: impl Into<String>) {
        self.clipboard = text.into();
    }

    /// Install the app's native menu model.
    pub fn set_menu(&mut self, menu: crate::system::MenuModel) {
        self.menu = menu;
    }

    /// The current menu model.
    pub fn menu(&self) -> &crate::system::MenuModel {
        &self.menu
    }

    /// Invoke a menu command by id; returns its label if it exists and is
    /// enabled, recording the invocation for the app/agent.
    pub fn invoke_menu(&mut self, id: &str) -> Option<String> {
        let label = self
            .menu
            .find(id)
            .filter(|i| i.enabled)
            .map(|i| i.label.clone())?;
        self.invoked_menu.push(id.to_string());
        Some(label)
    }

    /// Menu command ids invoked so far.
    pub fn invoked_menu(&self) -> &[String] {
        &self.invoked_menu
    }

    /// Record a request to an OS service (the real shell fulfils it).
    pub fn request_system(&mut self, req: crate::system::SystemRequest) {
        self.system_requests.push(req);
    }

    /// System requests recorded this session.
    pub fn system_requests(&self) -> &[crate::system::SystemRequest] {
        &self.system_requests
    }

    /// Declare the app's secondary windows (multi-window).
    pub fn set_windows(&mut self, windows: Vec<crate::system::WindowDesc>) {
        self.windows = windows;
    }

    /// The app's secondary windows.
    pub fn windows(&self) -> &[crate::system::WindowDesc] {
        &self.windows
    }

    /// Set the layout direction (T5.3). `true` mirrors the layout right-to-left
    /// for RTL locales; re-lays-out immediately.
    pub fn set_rtl(&mut self, rtl: bool) {
        self.rtl = rtl;
        self.rebuild();
    }

    /// Whether the layout is mirrored right-to-left.
    pub fn is_rtl(&self) -> bool {
        self.rtl
    }

    /// The semantics document (typed).
    pub fn semantics_doc(&self) -> SemanticsDoc {
        let focused = self.focused_node().map(|n| n.index());
        let root = self
            .sem_root
            .clone()
            .unwrap_or_else(|| SemanticsNode::new(0, Role::Window));
        SemanticsDoc {
            window: WindowInfo {
                width: self.size.width,
                height: self.size.height,
                scale: 1.0,
                focused,
            },
            root,
        }
    }

    // --- event routing ------------------------------------------------------

    fn route(&mut self, ev: Event) {
        match ev {
            Event::PointerDown(pe) => {
                // Bubble from the hit target up its ancestors, firing the
                // nearest focus/click/drag handlers (decorative children let
                // their interactive ancestor handle the press).
                let mut n = self.tree.hit_test(pe.pos);
                let (mut did_focus, mut did_click, mut did_drag) = (false, false, false);
                while let Some(node) = n {
                    if let Some(m) = self.meta.get(&node) {
                        if !did_focus && m.focusable {
                            self.focused_id = m.id.clone();
                            did_focus = true;
                        }
                        if !did_click {
                            if let Some(h) = m.on_click.clone() {
                                h(&self.rt);
                                did_click = true;
                            }
                        }
                        if !did_drag && m.on_drag.is_some() {
                            self.pressed = Some(node);
                            self.apply_drag(node, pe.pos);
                            did_drag = true;
                        }
                    }
                    if did_focus && did_click && did_drag {
                        break;
                    }
                    let p = self.tree.parent(node);
                    n = p.is_some().then_some(p);
                }
            }
            Event::PointerUp(_) => {
                self.pressed = None;
            }
            Event::TextInput(te) => {
                if let Some(node) = self.focused_node() {
                    if let Some(h) = self.meta.get(&node).and_then(|m| m.on_text.clone()) {
                        h(&self.rt, &te.text);
                    }
                }
            }
            Event::PointerMove(pe) => {
                let (_l, _e) = self.pointer.update(&self.tree, pe.pos);
                // Hover bubbles to the nearest ancestor with an id (like clicks
                // bubble to an on_click ancestor), so hovering a button's child
                // label still marks the button itself as hovered.
                let mut n = self.tree.hit_test(pe.pos);
                let mut id = None;
                while let Some(node) = n {
                    if let Some(m) = self.meta.get(&node) {
                        if m.id.is_some() {
                            id = m.id.clone();
                            break;
                        }
                    }
                    let p = self.tree.parent(node);
                    n = p.is_some().then_some(p);
                }
                self.hovered_id = id;
                if let Some(node) = self.pressed {
                    self.apply_drag(node, pe.pos);
                }
            }
            Event::Wheel(we) => {
                // Find the nearest ancestor (incl. target) with a wheel handler.
                let mut n = self.tree.hit_test(we.pos);
                while let Some(node) = n {
                    if let Some(h) = self.meta.get(&node).and_then(|m| m.on_wheel.clone()) {
                        h(&self.rt, we.delta.y);
                        break;
                    }
                    let parent = self.tree.parent(node);
                    n = parent.is_some().then_some(parent);
                }
            }
            Event::Drop(de) => {
                // Bubble to the nearest ancestor (incl. target) with a drop handler.
                let mut n = self.tree.hit_test(de.pos);
                while let Some(node) = n {
                    if let Some(h) = self.meta.get(&node).and_then(|m| m.on_drop.clone()) {
                        h(&self.rt, &de.data);
                        break;
                    }
                    let parent = self.tree.parent(node);
                    n = parent.is_some().then_some(parent);
                }
            }
            Event::KeyDown(ke) => match ke.key {
                Key::Named(NamedKey::Tab) => {
                    let forward = !ke.modifiers.contains(lumen_core::events::Modifiers::SHIFT);
                    self.move_focus(forward);
                }
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                    self.activate_focused();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn focused_node(&self) -> Option<NodeIndex> {
        let id = self.focused_id.as_ref()?;
        self.tree
            .document_order()
            .into_iter()
            .find(|n| self.meta.get(n).and_then(|m| m.id.as_ref()) == Some(id))
    }

    fn move_focus(&mut self, forward: bool) {
        let current = self.focused_node();
        if let Some(next) = lumen_core::events::next_focus(&self.tree, current, forward) {
            self.focused_id = self.meta.get(&next).and_then(|m| m.id.clone());
        }
    }

    fn activate_focused(&mut self) {
        if let Some(n) = self.focused_node() {
            if let Some(h) = self.meta.get(&n).and_then(|m| m.on_click.clone()) {
                h(&self.rt);
            }
        }
    }

    /// Call a node's drag handler with the pointer's fraction along its width.
    fn apply_drag(&self, node: NodeIndex, pos: Point) {
        let b = self.tree.bounds(node);
        if b.width() <= 0.0 {
            return;
        }
        let frac = ((pos.x - b.x0) / b.width()).clamp(0.0, 1.0);
        if let Some(h) = self.meta.get(&node).and_then(|m| m.on_drag.clone()) {
            h(&self.rt, frac);
        }
    }

    // --- rebuild ------------------------------------------------------------

    /// Rebuild, containing any panic in the build/layout/paint so a buggy frame
    /// can't take down the window (C2 / T7.3). On panic the previous good frame
    /// is kept and a structured `E0701` diagnostic is recorded; a clean build
    /// clears it.
    fn rebuild(&mut self) {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.rebuild_inner()));
        match result {
            Ok(()) => self.build_panic = None,
            Err(payload) => {
                let msg = panic_msg(&payload);
                self.build_panic = Some(lumen_core::Diagnostic::new(
                    lumen_core::codes::E0701,
                    format!("build panicked (frame contained): {msg}"),
                ));
            }
        }
    }

    fn rebuild_inner(&mut self) {
        let (root_el, requests) = {
            let mut cx = BuildCx::new(&self.rt, self.clock_ms);
            let el = (self.root)(&mut cx);
            (el, cx.take_requests())
        };
        self.requests = requests;

        let mut tree = Tree::new();
        let mut layout = LayoutTree::new();
        let mut meta = HashMap::new();
        let mut built: Vec<(NodeIndex, LayoutNode)> = Vec::new();
        let (_root_node, root_lnode) =
            self.build_node(root_el, &mut tree, &mut layout, &mut meta, &mut built, None);

        layout.compute(root_lnode, self.size);
        if self.rtl {
            layout.mirror_rtl(root_lnode);
        }
        for (node, lnode) in &built {
            tree.set_bounds(*node, layout.bounds(*lnode));
        }

        self.tree = tree;
        self.meta = meta;
        self.compute_styles();
        self.frame = self.paint();
        self.sem_root = Some(self.build_semantics(self.tree.root()));
    }

    /// Resolve the `.lss` cascade for every node, storing both the typed `Style`
    /// (applied to paint) and the raw computed values (for `get_styles`).
    fn compute_styles(&mut self) {
        self.node_style.clear();
        self.node_computed.clear();
        let Some(sheet) = &self.app_sheet else {
            return;
        };
        let sources = [lumen_style::StyleSource {
            origin: lumen_style::Origin::App,
            sheet: sheet.clone(),
        }];
        let tokens = lumen_style::tokens_for(sheet, self.theme);
        for node in self.tree.document_order() {
            let Some(m) = self.meta.get(&node) else {
                continue;
            };
            let mut states = Vec::new();
            let f = self.tree.flags(node);
            if f.contains(NodeFlags::FOCUSED) {
                states.push("focused".to_string());
            }
            if f.contains(NodeFlags::HOVERED) {
                states.push("hovered".to_string());
            }
            let desc = lumen_style::NodeDesc {
                id: m.id.as_ref().map(|i| i.as_str().to_string()),
                classes: m.classes.clone(),
                states,
                ty: m.role.as_str().to_string(),
            };
            let computed = lumen_style::resolve(&sources, &desc);
            let mut style = lumen_style::Style::new();
            let mut resolved = HashMap::new();
            for (prop, c) in &computed {
                lumen_style::apply(&mut style, prop, &c.value, &tokens);
                // Store the token-resolved value so `get_styles` returns the
                // computed (substituted) form (04 §7).
                resolved.insert(
                    prop.clone(),
                    lumen_style::Computed {
                        value: lumen_style::resolve_token(&c.value, &tokens),
                        important: c.important,
                        origin: c.origin,
                    },
                );
            }
            self.node_style.insert(node, style);
            self.node_computed.insert(node, resolved);
        }
    }

    /// Set/replace the app stylesheet at runtime (tier-1 hot reload). A broken
    /// edit is rejected and the previous stylesheet stays live (04 §9).
    pub fn set_stylesheet(&mut self, src: &str) -> ReloadResult {
        let (sheet, diags) = lumen_style::parse("app.lss", src);
        if lumen_style::has_errors(&diags) {
            ReloadResult::Failed(diags)
        } else {
            self.app_sheet = Some(sheet);
            self.rebuild();
            ReloadResult::Ok
        }
    }

    /// Switch the active theme and re-resolve styles.
    pub fn set_theme(&mut self, theme: lumen_style::ThemeKind) {
        self.theme = theme;
        self.rebuild();
    }

    /// Set the theme by name (`"light"|"dark"|"high-contrast"`).
    pub fn set_theme_str(&mut self, theme: &str) {
        let t = match theme {
            "dark" => lumen_style::ThemeKind::Dark,
            "high-contrast" => lumen_style::ThemeKind::HighContrast,
            _ => lumen_style::ThemeKind::Light,
        };
        self.set_theme(t);
    }

    /// Computed styles for the node a `selector` resolves to (03 §3 ui.getStyles,
    /// 04 §7 value serialization). Returns `null` if the selector doesn't resolve
    /// to exactly one node.
    pub fn get_styles(&self, selector: &str) -> serde_json::Value {
        let root = self.semantics_doc().root.elided();
        let Ok(id) = lumen_core::semantics::resolve_one(&root, selector) else {
            return serde_json::Value::Null;
        };
        // Map the semantics node id back to a live NodeIndex.
        let node = self
            .tree
            .document_order()
            .into_iter()
            .find(|n| n.index() == id);
        let Some(node) = node else {
            return serde_json::Value::Null;
        };
        let mut map = serde_json::Map::new();
        if let Some(computed) = self.node_computed.get(&node) {
            for (prop, c) in computed {
                map.insert(prop.clone(), lumen_style::computed_json(&c.value, c.origin));
            }
        }
        serde_json::Value::Object(map)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_node(
        &mut self,
        el: Element,
        tree: &mut Tree,
        layout: &mut LayoutTree,
        meta: &mut HashMap<NodeIndex, NodeMeta>,
        built: &mut Vec<(NodeIndex, LayoutNode)>,
        parent: Option<NodeIndex>,
    ) -> (NodeIndex, LayoutNode) {
        let node = match parent {
            None => tree.insert_root(),
            Some(p) => tree.insert_child(p),
        };

        let mut flags = NodeFlags::VISIBLE;
        let interactive = el.background.is_some()
            || el.on_click.is_some()
            || matches!(
                el.content,
                NodeContent::Text(..) | NodeContent::Image(..) | NodeContent::Custom(..)
            )
            || el.on_wheel.is_some()
            || el.on_drag.is_some();
        if interactive {
            flags |= NodeFlags::HIT_TESTABLE;
        }
        if el.focusable {
            flags |= NodeFlags::FOCUSABLE;
        }
        if el.id.is_some() && el.id == self.focused_id {
            flags |= NodeFlags::FOCUSED;
        }
        if el.id.is_some() && el.id == self.hovered_id {
            flags |= NodeFlags::HOVERED;
        }
        tree.set_flags(node, flags);

        // Text nodes get a fixed size from measurement.
        let mut style = el.style;
        let (pl, pt) = (dim_px(style.padding.left), dim_px(style.padding.top));
        let (pr, pb) = (dim_px(style.padding.right), dim_px(style.padding.bottom));
        let pad = (pl, pt);
        if let NodeContent::Text(txt, ts) = &el.content {
            // Size the box to text *plus* padding so the label has room; it's
            // then painted at the padded origin (centred for symmetric padding).
            let block = self
                .text
                .layout(txt, *ts, &[], None, lumen_text::TextAlign::Start);
            style.width = Dim::px(block.width().ceil() + (pl + pr) as f32);
            style.height = Dim::px(block.height().ceil() + (pt + pb) as f32);
        } else if let NodeContent::Custom(w) = &el.content {
            // Size a custom leaf from its intrinsic measure (E2).
            let s = w.measure(kurbo::Size::new(f64::INFINITY, f64::INFINITY));
            style.width = Dim::px(s.width.max(0.0) as f32);
            style.height = Dim::px(s.height.max(0.0) as f32);
        }

        // Consume the children (move, not clone) and recurse.
        let child_built: Vec<(NodeIndex, LayoutNode)> = el
            .children
            .into_iter()
            .map(|c| self.build_node(c, tree, layout, meta, built, Some(node)))
            .collect();
        let child_lnodes: Vec<LayoutNode> = child_built.iter().map(|(_, l)| *l).collect();
        let lnode = if child_lnodes.is_empty() {
            layout.leaf(style)
        } else {
            layout.container(style, &child_lnodes)
        };

        // Move the remaining fields into the retained NodeMeta (no clones).
        meta.insert(
            node,
            NodeMeta {
                id: el.id,
                role: el.role,
                label: el.label,
                value: el.value,
                classes: el.classes,
                actions: el.actions,
                states: el.states,
                scroll: el.scroll,
                focusable: el.focusable,
                elide: el.elide_semantics,
                on_click: el.on_click,
                on_wheel: el.on_wheel,
                on_drag: el.on_drag,
                on_drop: el.on_drop,
                on_text: el.on_text,
                background: el.background,
                corner_radius: el.corner_radius,
                shadow: el.shadow,
                content: el.content,
                pad,
            },
        );
        built.push((node, lnode));
        (node, lnode)
    }

    // --- paint --------------------------------------------------------------

    fn build_display_list(&mut self) -> (DisplayList, Vec<lumen_render::TextTarget>) {
        let mut dl = DisplayList::new();
        let mut text_targets: Vec<lumen_render::TextTarget> = Vec::new();
        let order = self.tree.document_order();
        for node in order {
            let bounds = self.tree.bounds(node);
            let Some(m) = self.meta.get(&node) else {
                continue;
            };
            // `.lss` overrides the widget's hardcoded background/radius.
            let css = self.node_style.get(&node);
            let mut bg = css.and_then(|s| s.background).or(m.background);
            // Hover feedback: lighten a dark control / darken a light one while
            // the pointer is over a clickable node. Automatic for every button.
            if let Some(c) = bg {
                if m.on_click.is_some() && self.tree.flags(node).contains(NodeFlags::HOVERED) {
                    bg = Some(hover_tint(c));
                }
            }
            let radius = css
                .and_then(|s| s.border_radius)
                .map(|r| r as f64)
                .unwrap_or(m.corner_radius);
            // Drop shadow: a soft penumbra approximated by stacked translucent
            // rounded rects. The stack is static for a given box, so rasterize
            // it once into a cached sprite and blit that each frame — otherwise
            // (8 large translucent fills) it dominates frame time.
            if let Some(sh) = m.shadow {
                let w = bounds.width();
                let h = bounds.height();
                let margin = (sh.spread.max(0.0) + sh.blur).ceil() + 2.0;
                let [r, g, b, a] = sh.color.to_srgb8();
                let key = (
                    (w * 4.0).round() as i32,
                    (h * 4.0).round() as i32,
                    (radius * 4.0).round() as i32,
                    (sh.blur * 4.0).round() as i32,
                    (sh.spread * 4.0).round() as i32,
                    u32::from_le_bytes([r, g, b, a]),
                );
                let sprite = if let Some(c) = self.shadow_cache.get(&key) {
                    c.clone()
                } else {
                    let sw = (w + 2.0 * margin).ceil() as u32;
                    let sh_px = (h + 2.0 * margin).ceil() as u32;
                    let mut sdl = DisplayList::new();
                    let base = Rect::new(margin, margin, margin + w, margin + h)
                        .inflate(sh.spread, sh.spread);
                    for i in 0..8u32 {
                        let frac = i as f64 / 8.0; // 0 (outer) .. ~1 (inner)
                        let grow = sh.blur * (1.0 - frac);
                        let alpha = (a as f64 * frac * frac / 2.0).round() as u8;
                        if alpha == 0 {
                            continue;
                        }
                        sdl.push(DrawCmd::Rect {
                            rect: base.inflate(grow, grow),
                            brush: Brush::Solid(Color::srgb8(r, g, b, alpha)),
                            radii: CornerRadii::all(radius + grow),
                            border: None,
                        });
                    }
                    let img = cpu::render(&sdl, sw.max(1), sh_px.max(1), Color::TRANSPARENT);
                    const CAP: usize = 64;
                    if self.shadow_cache.len() >= CAP {
                        self.shadow_cache.clear();
                    }
                    self.shadow_cache.insert(key, img.clone());
                    img
                };
                let iw = sprite.width() as f64;
                let ih = sprite.height() as f64;
                let id = lumen_render::ImageId(dl.images.len() as u32);
                dl.images.push(sprite);
                // Integer placement + nearest sampling makes each blit a straight
                // 1:1 copy (no resampling); a sub-pixel shadow shift is invisible.
                let px = (bounds.x0 + sh.dx - margin).round();
                let py = (bounds.y0 + sh.dy - margin).round();
                // The opaque box bg (drawn next) covers the box interior, so blit
                // only the surrounding penumbra: skip the largest rect provably
                // under the rounded bg (box inset by its radius) and emit the rest
                // as 4 bands. This is the frame's most expensive blit, and the
                // interior is ~half its pixels.
                let sx0 = (bounds.x0 - px + radius).ceil().clamp(0.0, iw);
                let sy0 = (bounds.y0 - py + radius).ceil().clamp(0.0, ih);
                let sx1 = (bounds.x0 - px + w - radius).floor().clamp(0.0, iw);
                let sy1 = (bounds.y0 - py + h - radius).floor().clamp(0.0, ih);
                let mut band = |x0: f64, y0: f64, x1: f64, y1: f64| {
                    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                        return;
                    }
                    dl.push(DrawCmd::Image {
                        id,
                        src_rect: Rect::new(x0, y0, x1, y1),
                        dst_rect: Rect::new(px + x0, py + y0, px + x1, py + y1),
                        quality: lumen_render::Filter::Nearest,
                    });
                };
                if sx1 > sx0 && sy1 > sy0 {
                    band(0.0, 0.0, iw, sy0); // top
                    band(0.0, sy1, iw, ih); // bottom
                    band(0.0, sy0, sx0, sy1); // left
                    band(sx1, sy0, iw, sy1); // right
                } else {
                    band(0.0, 0.0, iw, ih); // box too small to carve a hole
                }
            }
            if let Some(bg) = bg {
                dl.push(DrawCmd::Rect {
                    rect: bounds,
                    brush: Brush::Solid(bg),
                    radii: CornerRadii::all(radius),
                    border: None,
                });
            }
            // Immediate-mode canvas: draw in node-local coords offset to bounds.
            if let NodeContent::Canvas(draw) = &m.content {
                let mut frame = lumen_render::canvas::Frame::new(kurbo::Affine::translate((
                    bounds.x0, bounds.y0,
                )));
                draw(
                    &mut frame,
                    kurbo::Size::new(bounds.width(), bounds.height()),
                );
                for cmd in frame.into_cmds() {
                    dl.push(cmd);
                }
            }
            if let NodeContent::Custom(w) = &m.content {
                // Paint a custom leaf via the same node-local Frame as Canvas (E2).
                let mut frame = lumen_render::canvas::Frame::new(kurbo::Affine::translate((
                    bounds.x0, bounds.y0,
                )));
                w.paint(
                    &mut frame,
                    kurbo::Size::new(bounds.width(), bounds.height()),
                );
                for cmd in frame.into_cmds() {
                    dl.push(cmd);
                }
            }
            if let NodeContent::Image(img) = &m.content {
                let iw = img.width() as f64;
                let ih = img.height() as f64;
                let id = lumen_render::ImageId(dl.images.len() as u32);
                dl.images.push(img.clone());
                dl.push(DrawCmd::Image {
                    id,
                    src_rect: Rect::new(0.0, 0.0, iw, ih),
                    dst_rect: bounds,
                    quality: lumen_render::Filter::Nearest,
                });
            }
            if let NodeContent::Text(txt, ts) = &m.content {
                // Apply a `.lss` text colour to the glyphs (the cascade also
                // drives background/radius above). Colour is size-neutral, so it
                // doesn't desync the layout box measured at build time; `.lss`
                // font-size/weight on text remain follow-on (they'd need the
                // measure pass to consult the cascade too).
                let mut ts = *ts;
                if let Some(c) = css.and_then(|s| s.color) {
                    ts.color = c;
                }
                // Reuse a previously rasterized glyph image when the string and
                // style are unchanged (the common case across animation frames).
                let [cr, cg, cb, ca] = ts.color.to_srgb8();
                let key = (
                    txt.clone(),
                    ts.font_size.to_bits(),
                    ts.weight.to_bits(),
                    u32::from_le_bytes([cr, cg, cb, ca]),
                );
                let img = if let Some(cached) = self.text_cache.get(&key) {
                    cached.clone()
                } else {
                    let block = self
                        .text
                        .layout(txt, ts, &[], None, lumen_text::TextAlign::Start);
                    let img = block.render(0, 0, Color::srgb8(255, 255, 255, 0)); // transparent bg
                    const CAP: usize = 512;
                    if self.text_cache.len() >= CAP {
                        self.text_cache.clear();
                    }
                    self.text_cache.insert(key, img.clone());
                    img
                };
                let iw = img.width() as f64;
                let ih = img.height() as f64;
                let id = lumen_render::ImageId(dl.images.len() as u32);
                dl.images.push(img);
                // Paint at the padded (content-box) origin so a button label
                // sits inside its padding (centred for symmetric padding) rather
                // than jammed into the border-box corner. Plain text has no
                // padding, so this is a no-op for it.
                let tx = bounds.x0 + m.pad.0;
                let ty = bounds.y0 + m.pad.1;
                dl.push(DrawCmd::Image {
                    id,
                    src_rect: Rect::new(0.0, 0.0, iw, ih),
                    dst_rect: Rect::new(tx, ty, tx + iw, ty + ih),
                    quality: lumen_render::Filter::Nearest,
                });
                // Mirror the painted text as a design-analysis target: the
                // foreground is the text's resolved color, the region its bounds.
                text_targets.push(lumen_render::TextTarget {
                    node: Some(format!("node-{}", node.index())),
                    label: Some(txt.clone()),
                    foreground: ts.color,
                    region: bounds,
                });
            }
        }
        (dl, text_targets)
    }

    fn paint(&mut self) -> RgbaImage {
        let (dl, _) = self.build_display_list();
        // Layout/display list are in logical px; rasterize at physical px so the
        // frame matches a HiDPI surface 1:1 (no upscaling blur). scale 1.0 is
        // byte-identical to the unscaled path (goldens unaffected).
        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
        self.renderer
            .render_frame(&dl, pw, ph, self.scale, Color::srgb8(255, 255, 255, 255))
    }

    /// Swap the frame renderer backend (A1 — the runtime is generic over it).
    /// Defaults to the CPU reference renderer.
    pub fn set_renderer(&mut self, renderer: Box<dyn lumen_render::Renderer>) {
        self.renderer = renderer;
        self.pump();
    }

    /// The active renderer backend's name (e.g. `"cpu"`).
    pub fn renderer_name(&self) -> &'static str {
        self.renderer.name()
    }

    /// A deterministic APCA text-contrast report over the current frame's
    /// display list (prototype design-analysis surface, ADR pending). Each
    /// finding is bound to the `node-<index>` id of the text node it assesses,
    /// and contrast is measured against the *composited* backdrop.
    pub fn contrast_report(&mut self) -> lumen_render::ContrastReport {
        let (dl, targets) = self.build_display_list();
        lumen_render::analyze_contrast(&dl, Color::srgb8(255, 255, 255, 255), &targets)
    }

    // --- semantics ----------------------------------------------------------

    fn build_semantics(&self, node: NodeIndex) -> SemanticsNode {
        let m = self.meta.get(&node);
        let mut s = SemanticsNode::new(node.index(), m.map(|m| m.role).unwrap_or(Role::Generic));
        if let Some(m) = m {
            s.id = m.id.clone();
            s.label = m.label.clone();
            s.value = m.value.clone();
            s.classes = m.classes.clone();
            s.actions = m.actions.clone();
            s.type_name = format!("{:?}", m.role);
            s.elide = m.elide;
            s.scroll = m.scroll;
            s.states = m.states.clone();
            let flags = self.tree.flags(node);
            if flags.contains(NodeFlags::FOCUSED) {
                s.states.push(SemState::Focused);
            }
            if flags.contains(NodeFlags::HOVERED) {
                s.states.push(SemState::Hovered);
            }
            if flags.contains(NodeFlags::DISABLED) {
                s.states.push(SemState::Disabled);
            }
        }
        s.bounds = self.tree.bounds(node);
        let mut child = self.tree.first_child(node);
        while child.is_some() {
            s.children.push(self.build_semantics(child));
            child = self.tree.next_sibling(child);
        }
        s
    }
}

/// Extract a human-readable message from a caught panic payload.
fn panic_msg(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_string()
    }
}

/// A hover-state version of a control colour: lighten a dark fill, darken a
/// light one (perceptually, in Oklab). Subtle but visible.
fn hover_tint(c: Color) -> Color {
    let lum = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    let target = if lum < 0.5 {
        Color::WHITE
    } else {
        Color::BLACK
    };
    c.lerp_oklab(target, 0.12)
}

/// Helper: the center point of a rect (for synthesized clicks).
pub fn center(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

/// Re-export so callers can build the default window background.
pub const WINDOW_BG: Color = Color::WHITE;

/// A default style alias used by examples.
pub type Style = LayoutStyle;
