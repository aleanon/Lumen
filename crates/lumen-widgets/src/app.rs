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
use lumen_render::{
    cpu, BlendMode, Border, Brush, CornerRadii, Damage, DisplayList, DrawCmd, RgbaImage,
    RoundedRect,
};
use lumen_text::TextEngine;
use std::cell::RefCell;
use std::collections::HashMap;

/// Hit-test z for overlay subtrees (dropdown menus, popovers, tooltips). They
/// paint on top in a final pass, so they must also win hit-testing over the
/// normal-flow content they cover (which has the default z of 0).
const OVERLAY_Z: u32 = 1000;

/// Statistics for one rendered frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameStats {
    /// Number of live nodes after the rebuild.
    pub node_count: usize,
    /// Whether any pixels were repainted this frame (`false` = idle frame, the
    /// previous frame was reused verbatim).
    pub painted: bool,
    /// What changed this frame (R2): `None` (idle), `Region` (only a rectangle
    /// repainted), or `Full`. The shell can upload just the changed region.
    pub damage: Damage,
}

/// An application: a root build closure, an optional stylesheet, and the frame
/// renderer backend `R` (defaults to [`lumen_render::DefaultRenderer`] = the
/// deterministic CPU `TinySkia`). The runtime is generic over `R` — zero-cost by
/// default; a consumer who wants dynamic backend selection uses
/// `R = Box<dyn Renderer>` (see the blanket `Renderer` impl in `lumen-render`).
pub struct App<R = lumen_render::DefaultRenderer, E = lumen_core::tasks::InlineSpawner> {
    root: Box<dyn Fn(&mut BuildCx) -> Element>,
    #[allow(dead_code)]
    stylesheet: Option<String>,
    /// Extra fonts to register at boot (B1): app-provided bytes, selected by
    /// family name via `TextStyle::family`. The bundled font stays the default.
    fonts: Vec<Vec<u8>>,
    renderer: R,
    executor: E,
}

impl App<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner> {
    /// Create an app from its root build closure (02 §8), on the default CPU
    /// reference renderer and the deterministic inline executor.
    pub fn new(root: impl Fn(&mut BuildCx) -> Element + 'static) -> App {
        App {
            root: Box::new(root),
            stylesheet: None,
            fonts: Vec::new(),
            renderer: lumen_render::TinySkia,
            executor: lumen_core::tasks::InlineSpawner,
        }
    }
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner> App<R, E> {
    /// Attach a stylesheet (parsed in M1; stored for now).
    pub fn stylesheet(mut self, lss: &str) -> App<R, E> {
        self.stylesheet = Some(lss.to_string());
        self
    }

    /// Register an extra font (its bytes) for the app, selectable by family name
    /// via [`TextStyle::family`](lumen_text::TextStyle::family). Additive — the
    /// bundled font stays the default; no system-font enumeration (ADR-005).
    pub fn with_font(mut self, bytes: impl Into<Vec<u8>>) -> App<R, E> {
        self.fonts.push(bytes.into());
        self
    }

    /// Swap the frame renderer backend, changing the app's `R` type (typestate
    /// builder). The CPU reference renderer is the default; the shell hands in a
    /// GPU backend (constructed post-surface), and a consumer wanting runtime
    /// selection passes a `Box<dyn Renderer>`.
    pub fn with_renderer<R2: lumen_render::Renderer>(self, renderer: R2) -> App<R2, E> {
        App {
            root: self.root,
            stylesheet: self.stylesheet,
            fonts: self.fonts,
            renderer,
            executor: self.executor,
        }
    }

    /// Swap the background-work executor, changing the app's `E` type (typestate
    /// builder). Defaults to the deterministic [`InlineSpawner`](lumen_core::tasks::InlineSpawner);
    /// the shell hands in a real thread-pool / async executor, and a consumer
    /// wanting runtime selection passes a `Box<dyn Spawner>`.
    pub fn with_executor<E2: lumen_core::tasks::Spawner>(self, executor: E2) -> App<R, E2> {
        App {
            root: self.root,
            stylesheet: self.stylesheet,
            fonts: self.fonts,
            renderer: self.renderer,
            executor,
        }
    }

    /// Run headless at `size` (no OS dependencies).
    pub fn run_headless(self, size: Size) -> Headless<R, E> {
        let mut h = self.into_headless(size, None);
        h.rebuild();
        h
    }

    /// Run headless, restoring a prior [`AppSnapshot`] (tier-3 restart,
    /// ADR-011). Returns the instance plus any `W0002` drop diagnostics raised
    /// when a snapshot value no longer has a matching signal. Snapshot builds
    /// only.
    #[cfg(feature = "snapshot")]
    pub fn run_headless_restored(
        self,
        size: Size,
        snap: AppSnapshot,
    ) -> (Headless<R, E>, Vec<lumen_core::Diagnostic>) {
        // Focus is host state (not in the reactive store), so it is carried on
        // the snapshot and re-applied directly.
        let mut h = self.into_headless(size, snap.focused.clone());
        // Stage the snapshot *before* the first build so each signal adopts its
        // restored value as it is (re-)created (Checkpoint protocol).
        h.rt.load_pending(snap.state);
        h.rebuild();
        let diags = h.rt.finish_restore();
        (h, diags)
    }

    /// Construct the headless instance (fonts registered, focus applied) without
    /// the first build. Shared by the plain and restore boot paths.
    fn into_headless(self, size: Size, focused: Option<StableId>) -> Headless<R, E> {
        // Register app fonts before the first build so styled text can select them.
        let mut text = TextEngine::new();
        for bytes in self.fonts {
            text.register_font(bytes);
        }
        let h = Headless {
            root: self.root,
            rt: Runtime::new(),
            size,
            scale: 1.0,
            clock_ms: 0.0,
            renderer: self.renderer,
            executor: self.executor,
            task_waker: None,
            text,
            text_cache: HashMap::new(),
            shadow_cache: HashMap::new(),
            tree: Tree::new(),
            meta: HashMap::new(),
            node_ink: HashMap::new(),
            node_text_metrics: HashMap::new(),
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
            style_env: None,
            scope_spans: HashMap::new(),
            desc_stack: Vec::new(),
            container_nodes: Vec::new(),
            container_prev: Vec::new(),
            container_stack: Vec::new(),
            container_repass: false,
            frame_ms: std::collections::VecDeque::new(),
            frames_rendered: 0,
            menu: crate::system::MenuModel::default(),
            invoked_menu: Vec::new(),
            system_requests: Vec::new(),
            windows: Vec::new(),
            rtl: false,
            last_dl: None,
            last_damage: lumen_render::Damage::Full,
            surface_attached: false,
            last_build_gen: 0,
            force_rebuild: false,
            last_build_clock: 0.0,
            scope_cache: RefCell::new(HashMap::new()),
            scope_live: RefCell::new(std::collections::HashSet::new()),
            bg_bindings: Vec::new(),
            structural_reads: lumen_core::state::ReadSet::default(),
            dep_index: HashMap::new(),
            last_change: ChangeReport {
                kind: "idle",
                nodes: Vec::new(),
            },
        };
        h
    }
}

/// A tier-3 snapshot of a running app: the reactive store (every signal —
/// including scroll offsets) plus focus. Serializable, so it can be written
/// before a process restart and restored afterwards (ADR-011). Snapshot builds
/// only.
#[cfg(feature = "snapshot")]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSnapshot {
    state: lumen_core::state::StateSnapshot,
    focused: Option<StableId>,
}

/// Checkpoint protocol (02 §4, ADR-011) — the tier-2/3 hot-reload contract,
/// formalized over the snapshot machinery (W.4b). `quiesce` parks the app at
/// a safe point (reactive graph at fixpoint), `serialize_state` captures the
/// store + host extras (focus), `restore_state` adopts a snapshot into the
/// **running** instance (existing signals restored in place, re-created ones
/// adopt on rebuild; returns `W0002` drop diagnostics), and `resume` repaints
/// from the restored state. Snapshot builds only.
#[cfg(feature = "snapshot")]
pub trait Checkpoint {
    /// Park at a safe point: drain scheduled reactive work to a fixpoint.
    fn quiesce(&mut self);
    /// Capture the entire store plus host extras (focus).
    fn serialize_state(&self) -> AppSnapshot;
    /// Adopt `snap` into the running instance, returning `W0002` diagnostics
    /// for snapshot values that no longer have a matching signal.
    fn restore_state(&mut self, snap: AppSnapshot) -> Vec<lumen_core::Diagnostic>;
    /// Resume presentation: repaint from the restored state.
    fn resume(&mut self);
}

#[cfg(feature = "snapshot")]
impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner> Checkpoint for Headless<R, E> {
    fn quiesce(&mut self) {
        // pump() flushes writes and asserts the graph is quiescent on exit.
        self.pump();
    }

    fn serialize_state(&self) -> AppSnapshot {
        self.snapshot()
    }

    fn restore_state(&mut self, snap: AppSnapshot) -> Vec<lumen_core::Diagnostic> {
        self.rt.load_pending(snap.state);
        // Existing slots adopt in place (schedules their subscribers) …
        let mut diags = self.rt.adopt_pending_live();
        // … focus is host state, re-applied directly …
        self.focused_id = snap.focused;
        // … and a forced rebuild lets conditionally-created signals adopt the
        // rest before leftovers become W0002 drops.
        self.force_rebuild = true;
        self.pump();
        diags.extend(self.rt.finish_restore());
        diags
    }

    fn resume(&mut self) {
        self.force_full_repaint();
    }
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

/// A retained paint-only prop binding (F3.4): its node, the binding, and the
/// signals it last read. When those change, the runtime re-evaluates the binding
/// and patches `meta[node]` + repaints — no rebuild, no relayout.
struct BoundBg {
    node: NodeIndex,
    dynamic: lumen_core::Dynamic<Color>,
    deps: lumen_core::state::ReadSet,
}

/// Per-node reactive dependencies, split by source (F4). The union projects to
/// `SemanticsNode.deps` (F2); the breakdown backs `ui.getDeps` and the reverse
/// index. `background` deps update via a paint-only patch; `scope`/`text` via a
/// rebuild (F3.4).
#[derive(Default, Clone)]
struct NodeDeps {
    scope: Vec<String>,
    text: Vec<String>,
    background: Vec<String>,
    class: Vec<String>,
}

impl NodeDeps {
    /// De-duplicated union of all sources (for `SemanticsNode.deps`).
    fn union(&self) -> Vec<String> {
        let mut d: Vec<String> = Vec::new();
        for k in self
            .scope
            .iter()
            .chain(&self.text)
            .chain(&self.background)
            .chain(&self.class)
        {
            if !d.contains(k) {
                d.push(k.clone());
            }
        }
        d
    }

    fn is_empty(&self) -> bool {
        self.scope.is_empty()
            && self.text.is_empty()
            && self.background.is_empty()
            && self.class.is_empty()
    }
}

/// A reverse-index entry (F4.2): a node that depends on some signal, and how it
/// would update when that signal changes.
#[derive(Clone)]
struct DepEntry {
    /// Node index (serialized as `node-<index>`).
    node: u32,
    /// Which prop the dependency is through: `scope` / `text` / `background`.
    via: &'static str,
    /// How a change propagates: `rebuild` (scope/text) or `patch` (background).
    update: &'static str,
}

/// Per-rebuild style-resolution environment (A.2): the cascade sources and
/// token table, computed once per rebuild and consumed inline by
/// `build_node` so `.lss` layout properties reach taffy *before* layout.
/// B.2: also carries the live [`lumen_style::MediaContext`] so `@media`
/// blocks gate on the real window instead of applying unconditionally.
struct StyleEnv {
    sources: [lumen_style::StyleSource; 1],
    tokens: lumen_style::Tokens,
    media: lumen_style::MediaContext,
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
    /// Per-prop signal dependencies (F2 union → semantics; F4 breakdown).
    deps: NodeDeps,
    on_click: Option<Handler>,
    on_wheel: Option<crate::element::WheelHandler>,
    on_drag: Option<crate::element::DragHandler>,
    on_drop: Option<crate::element::DropHandler>,
    on_text: Option<crate::element::TextHandler>,
    on_key: Option<crate::element::KeyHandler>,
    on_caret_set: Option<crate::element::CaretHandler>,
    caret_byte: Option<usize>,
    selection: Option<(usize, usize)>,
    on_dismiss: Option<Handler>,
    background: Option<Color>,
    border: Option<Border>,
    corner_radius: f64,
    clip: bool,
    overlay: bool,
    shadow: Option<crate::element::Shadow>,
    content: NodeContent,
    /// Left/top padding in px — own-text is painted at the padded (content-box)
    /// origin, so a button label sits inside its padding instead of jammed into
    /// the border-box corner.
    pad: (f64, f64),
    /// Content-box wrap width in px for a wrapping text paragraph (set when the
    /// element carries an explicit pixel width). `None` = size-to-content, no
    /// wrap. The paint pass must lay out with the same width as the measure pass.
    wrap_width: Option<f32>,
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
pub struct Headless<R = lumen_render::DefaultRenderer, E = lumen_core::tasks::InlineSpawner> {
    root: Box<dyn Fn(&mut BuildCx) -> Element>,
    rt: Runtime,
    /// Logical size (the coordinate space for layout, events, and the display
    /// list). The rasterized frame is this times [`Headless::scale`].
    size: Size,
    /// HiDPI scale factor: the frame is rendered at `size * scale` physical px.
    scale: f64,
    clock_ms: f64,
    /// The frame renderer backend `R` (A1). The runtime is generic over it,
    /// chosen at construction (`App::with_renderer`); defaults to the CPU
    /// reference renderer. Zero-cost by default; `R = Box<dyn Renderer>` opts
    /// into dynamic dispatch.
    renderer: R,
    /// The background-work executor `E` (the data layer). Generic, chosen at
    /// construction (`App::with_executor`); defaults to the deterministic inline
    /// executor. `E = Box<dyn Spawner>` opts into dynamic dispatch.
    executor: E,
    /// Host waker: wakes the event loop when a background result is queued, so a
    /// frame gets scheduled. Set by the shell; `None` headless (the next manual
    /// `pump` drains the deferred-op queue).
    task_waker: Option<lumen_core::tasks::WakeFn>,
    text: TextEngine,
    /// Cache of rasterized text keyed by (string, size bits, weight bits, sRGB
    /// color): static labels then cost one memcpy per frame instead of a full
    /// reshape + glyph raster. Cleared wholesale when it exceeds a cap so an
    /// animated readout (many distinct strings) can't grow it without bound.
    text_cache: HashMap<(String, u32, u32, u32, u32), RgbaImage>,
    /// Cache of rasterized drop shadows keyed by quantized (w, h, radius, blur,
    /// spread, color). The stacked-rounded-rect penumbra is the single most
    /// expensive thing in a typical frame; since it's static for a given box it
    /// is rendered once and then blitted as one image.
    shadow_cache: HashMap<(i32, i32, i32, i32, i32, u32), RgbaImage>,
    tree: Tree,
    meta: HashMap<NodeIndex, NodeMeta>,
    /// Rendered *ink* bounds per node from the last paint — the union of what a
    /// node actually painted (text uses the glyph-ink `run_rect`, which can extend
    /// past the layout box via descenders/side bearings). Absent ⇒ ink == box.
    /// Drives the clipping audit (W0104) and `ui.getLayout`'s `ink`.
    node_ink: HashMap<NodeIndex, kurbo::Rect>,
    /// Typographic metrics per text node from the last paint (diagnostic aid;
    /// surfaced on `SemanticsNode.text_metrics` and via `ui.getLayout`).
    node_text_metrics: HashMap<NodeIndex, lumen_text::TextMetrics>,
    frame: RgbaImage,
    sem_root: Option<SemanticsNode>,
    /// If the last build panicked, the contained diagnostic (the previous good
    /// frame is kept). Cleared on the next successful build (C2 / T7.3).
    build_panic: Option<lumen_core::Diagnostic>,
    focused_id: Option<StableId>,
    hovered_id: Option<StableId>,
    /// The node being dragged: its index *and* stable id (if any). The id lets a
    /// drag survive rebuilds that renumber nodes (e.g. a scrollbar whose index
    /// shifts as list rows load) by re-resolving the current node each move.
    pressed: Option<(NodeIndex, Option<StableId>)>,
    app_sheet: Option<lumen_style::Stylesheet>,
    theme: lumen_style::ThemeKind,
    node_style: HashMap<NodeIndex, lumen_style::Style>,
    node_computed: HashMap<NodeIndex, HashMap<String, lumen_style::Computed>>,
    /// A.2: per-rebuild cascade env (None when no stylesheet is attached).
    style_env: Option<StyleEnv>,
    /// A.3.1: per-rebuild scope→node-span map (scope key → subtree root +
    /// preorder node count). The retained-graph splice replaces these spans.
    scope_spans: HashMap<String, (NodeIndex, u32)>,
    /// B.1: the ancestor descriptors of the element currently being lowered
    /// (root-first), fed to `resolve_with_ancestors` so descendant/`>`
    /// selectors match correctly. Maintained by `build_node`'s recursion.
    desc_stack: Vec<lumen_style::NodeDesc>,
    /// B.2b: container-query support. `container_nodes` — the `.container()`
    /// nodes of the current tree in build order; `container_prev` — their
    /// sizes from the last layout (what the build resolves against);
    /// `container_stack` — build-time stack of the nearest enclosing
    /// container's size (`None` = not yet measured); `container_repass` —
    /// re-entrancy guard for the bounded post-layout re-pass.
    container_nodes: Vec<NodeIndex>,
    container_prev: Vec<(f64, f64)>,
    container_stack: Vec<Option<(f64, f64)>>,
    container_repass: bool,
    /// C.2: rolling per-painted-frame pump durations in ms (cap 120) + total
    /// painted frames — the agent's `app.perf`. Diagnostic only (never feeds
    /// rendering); not recorded on wasm (no `Instant`).
    frame_ms: std::collections::VecDeque<f32>,
    /// C.2: total painted frames since launch.
    frames_rendered: u64,
    input: InputQueue,
    pointer: PointerState,
    // Animation/timer requests from the latest build (02 §8, time-driven UI).
    requests: crate::element::FrameRequests,
    // Desktop system integration (T5.2). The clipboard lives on the Runtime so
    // event handlers can reach it; see `Runtime::clipboard`.
    menu: crate::system::MenuModel,
    invoked_menu: Vec<String>,
    system_requests: Vec<crate::system::SystemRequest>,
    windows: Vec<crate::system::WindowDesc>,
    rtl: bool,
    /// Previous frame's display list, retained so the next paint can compute a
    /// damage region and repaint only what changed (R2). `None` forces a full
    /// repaint (first frame, or after a resize/scale change).
    last_dl: Option<DisplayList>,
    /// Damage applied by the most recent paint (reported via [`FrameStats`]).
    last_damage: lumen_render::Damage,
    /// True once a live window surface is wired to the renderer (1c). The build
    /// then presents straight to the swapchain instead of rasterizing to
    /// `self.frame`; `screenshot()` renders on demand. Always false headless.
    surface_attached: bool,
    /// `Runtime::write_gen()` captured after the last rebuild — `pump` compares it
    /// to the current value to detect whether any signal changed, and skips the
    /// rebuild entirely when nothing did (idle/non-effecting frames cost ~µs).
    last_build_gen: u64,
    /// Forces the next `pump` to rebuild regardless of reactive state — set by
    /// resize/scale/stylesheet/theme changes and `force_full_repaint`, which alter
    /// the frame without going through a signal.
    force_rebuild: bool,
    /// `clock_ms` at the last rebuild. If the last build read the clock
    /// (`requests.read_clock`), `pump` rebuilds whenever the clock has advanced
    /// past this — so time-driven UI updates even without an explicit `animate`/
    /// `wake_at`.
    last_build_clock: f64,
    /// Memoized `cx.scope` subtrees (F1), persisted across builds. A rebuild
    /// reuses a scope's cached subtree while none of the signals it read has
    /// changed; cleared by `clear_view_caches` (the oracle + non-signal
    /// rebuilds). Coherence is guarded by `assert_view_coherent` (F0).
    scope_cache: RefCell<crate::element::ScopeCache>,
    /// Scope keys accessed during the current build (F5 GC). After the build,
    /// cached scopes + scope-local signals whose key is absent are swept.
    scope_live: RefCell<std::collections::HashSet<String>>,
    /// Retained paint-only prop bindings from the last build (F3.4). A change to
    /// one binding's deps patches its node + repaints, skipping the rebuild.
    bg_bindings: Vec<BoundBg>,
    /// Signals whose change requires a structural rebuild (root + scope + text-
    /// binding reads; paint-only bindings are isolated out). `is_current` false ⇒
    /// rebuild; else a paint-only binding change can be patched (F3.4).
    structural_reads: lumen_core::state::ReadSet,
    /// Reverse index (F4.2): signal key → the nodes that depend on it and how
    /// they'd update. Rebuilt each `rebuild` from the per-node `NodeDeps`.
    dep_index: HashMap<String, Vec<DepEntry>>,
    /// What the last `pump` actually did (F4.3 change attribution).
    last_change: ChangeReport,
}

/// What a `pump` did, for change attribution (F4.3).
#[derive(Clone, Default)]
struct ChangeReport {
    /// `"idle"`, `"patch"` (paint-only bindings), or `"rebuild"` (structural).
    kind: &'static str,
    /// Node indices that were patched/rebuilt-with-changed-output this pump.
    nodes: Vec<u32>,
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner> Headless<R, E> {
    /// Process the input queue, then rebuild/layout/paint/semantics one turn —
    /// unless nothing that affects the frame changed, in which case the rebuild is
    /// skipped entirely (idle/non-effecting frames cost ~µs instead of ~ms).
    pub fn pump(&mut self) -> FrameStats {
        // C.2: time painted pumps for `app.perf`. Diagnostic-only wall time —
        // it never feeds rendering, so the pure-function contract holds.
        #[cfg(not(target_arch = "wasm32"))]
        let pump_t0 = std::time::Instant::now();
        // Apply any background-task results first (on the UI thread), so the build
        // sees fresh state. Keeps `pump` a pure function of (state, queued
        // events + deferred ops, clock). Deferred results write signals → bump the
        // reactive write-gen, which the skip check below observes.
        self.rt.drain_deferred();
        // Input-driven visual state that doesn't go through a signal (hover/focus/
        // pressed). Snapshot it to detect changes from routing.
        let visual_before = (
            self.hovered_id.clone(),
            self.focused_id.clone(),
            self.pressed.clone(),
        );
        let mut events = Vec::new();
        while let Some(ev) = self.input.pop() {
            events.push(ev);
        }
        for ev in events {
            self.route(ev);
        }
        // Rebuild only when something that affects the frame changed:
        //  - a signal/memo write since the last build (reactive write-gen),
        //  - input-driven visual state (hover/focus/pressed) changed,
        //  - the UI is time-driven this tick (continuous, or a one-shot wake came
        //    due since the last build),
        //  - or a forced invalidation (resize/scale/stylesheet/theme/repaint).
        // Conservative: anything uncertain forces a rebuild (set bumps the write-
        // gen even on equal values; any visual delta counts).
        let visual_changed = (
            self.hovered_id.clone(),
            self.focused_id.clone(),
            self.pressed.clone(),
        ) != visual_before;
        // Time-driven iff the last build read the clock (or asked to animate) AND
        // the clock has advanced since that build — then the frame would differ.
        let time_driven = (self.requests.read_clock || self.requests.continuous)
            && self.clock_ms != self.last_build_clock;
        let write_changed = self.rt.write_gen() != self.last_build_gen;
        // F3.4: a structural signal changed ⇒ rebuild; a change confined to
        // paint-only (background) bindings ⇒ patch that node + repaint, no
        // rebuild/relayout. `structural_reads` is every build-time read except
        // isolated paint-only bindings.
        let structural_current = self.structural_reads.is_current(&self.rt);
        let needs_rebuild = self.force_rebuild
            || visual_changed
            || time_driven
            || (write_changed && !structural_current);
        if needs_rebuild {
            // Scope memoization keys off signal versions only, so a rebuild
            // driven by a forced invalidation (resize/scale/stylesheet/theme —
            // inputs a build can observe through `cx`) must not reuse stale
            // subtrees: drop the caches and let this build repopulate.
            //
            // Visual state (hover/focus/pressed) deliberately does NOT clear
            // them (A.1, docs/plan-retained-pipeline.md): `BuildCx` exposes no
            // accessor for it, so no view closure can depend on it — it is
            // applied *after* the closures run (node flags in `build_node`,
            // `.lss` state parts in `compute_styles`, focus ring/caret in
            // paint), all of which a memoized rebuild re-does for every node
            // regardless. Pointer motion therefore gets F1-memoized rebuilds
            // instead of unmemoized O(tree) ones. Guarded by
            // tests/hover_memo.rs; if visual state ever becomes readable from
            // `BuildCx`, it must be signal-backed so scopes record the read.
            if self.force_rebuild {
                self.clear_view_caches();
            }
            self.rebuild(); // baselines force_rebuild + last_build_gen
        } else if write_changed
            && self
                .bg_bindings
                .iter()
                .any(|b| !b.deps.is_current(&self.rt))
        {
            self.patch_bg_bindings();
        } else {
            // Nothing changed — keep the retained frame, report no damage.
            self.last_damage = Damage::None;
            self.last_change = ChangeReport {
                kind: "idle",
                nodes: Vec::new(),
            };
        }
        // F0 fixpoint contract: a settled pump leaves the reactive graph
        // quiescent. Writes flush synchronously, so after dispatch + build
        // nothing should stay dirty; if this fires, some effect is scheduling
        // work that never drains (a real bug, not a perf issue).
        debug_assert!(
            self.rt.is_quiescent(),
            "pump left the reactive graph non-quiescent"
        );
        let stats = FrameStats {
            node_count: self.tree.len(),
            painted: self.last_damage != Damage::None,
            damage: self.last_damage,
        };
        #[cfg(not(target_arch = "wasm32"))]
        if stats.painted {
            let ms = pump_t0.elapsed().as_secs_f32() * 1000.0;
            if self.frame_ms.len() >= 120 {
                self.frame_ms.pop_front();
            }
            self.frame_ms.push_back(ms);
            self.frames_rendered += 1;
        }
        stats
    }

    /// C.2 (`app.perf`): rolling painted-frame time percentiles
    /// `(p50_ms, p95_ms)` over the last ≤120 painted pumps, plus the total
    /// painted-frame count. Zeros before anything painted (and on wasm).
    pub fn perf_stats(&self) -> (f64, f64, u64) {
        let mut v: Vec<f32> = self.frame_ms.iter().copied().collect();
        if v.is_empty() {
            return (0.0, 0.0, self.frames_rendered);
        }
        v.sort_by(f32::total_cmp);
        let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize] as f64;
        (pct(0.50), pct(0.95), self.frames_rendered)
    }

    /// Enqueue an event (OS or synthesized — same path).
    pub fn inject(&mut self, ev: Event) {
        self.input.push(ev);
    }

    /// W.0: the node a custom leaf's `event()` would be offered `ev` at —
    /// the hit-test target for pointer events, the focused node for
    /// keyboard/text. `None` for events leaves don't receive directly.
    fn leaf_event_target(&self, ev: &Event) -> Option<NodeIndex> {
        match ev {
            Event::PointerDown(pe) | Event::PointerUp(pe) | Event::PointerMove(pe) => {
                self.tree.hit_test(pe.pos)
            }
            Event::Wheel(we) => self.tree.hit_test(we.pos),
            Event::KeyDown(_) | Event::KeyUp(_) | Event::TextInput(_) => self.focused_node(),
            _ => None,
        }
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
            self.force_rebuild = true; // layout changed; not a signal write
            self.pump();
        }
    }

    /// Set logical size + HiDPI scale **without** repainting — the caller pumps
    /// once afterward. Lets the shell apply a coalesced resize and render the new
    /// size in a single pump per frame (instead of `resize()` + `set_scale()`
    /// each pumping, then another `pump()`). No-op-safe; ignores non-positive
    /// scale.
    pub fn prepare_resize(&mut self, size: Size, scale: f64) {
        if size != self.size || (scale > 0.0 && scale != self.scale) {
            self.force_rebuild = true; // make the following pump rebuild
        }
        self.size = size;
        if scale > 0.0 {
            self.scale = scale;
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
            self.force_rebuild = true; // physical raster size changed
            self.pump();
        }
    }

    /// The most recent rendered frame. With a live surface attached (1c) the
    /// build presents straight to the swapchain and no longer fills `self.frame`,
    /// so render the retained display list on demand here (the agent/test capture
    /// path — a freshly rendered frame of current state). Otherwise return the
    /// cached frame.
    pub fn screenshot(&mut self) -> RgbaImage {
        if self.surface_attached {
            if let Some(dl) = self.last_dl.take() {
                let pw = (self.size.width * self.scale).round().max(1.0) as u32;
                let ph = (self.size.height * self.scale).round().max(1.0) as u32;
                let bg = Color::srgb8(255, 255, 255, 255);
                let img = self.renderer.render_frame(&dl, pw, ph, self.scale, bg);
                self.last_dl = Some(dl);
                return img;
            }
        }
        self.frame.clone()
    }

    /// Render a magnified crop of `region` (logical px) at `scale_mul`× the normal
    /// scale, with optional debug `outlines` (rect + color, drawn as 1px borders)
    /// — e.g. a node's box and ink bounds. Lets a small defect (a clipped
    /// descender) be inspected at zoom instead of hunting for it in a full-window
    /// screenshot. Deterministic (same CPU/GPU render path); overlays are opt-in.
    pub fn screenshot_zoom(
        &mut self,
        region: kurbo::Rect,
        scale_mul: f64,
        outlines: &[(kurbo::Rect, Color)],
    ) -> RgbaImage {
        let (mut dl, _) = self.build_display_list();
        for (r, color) in outlines {
            dl.push(DrawCmd::Rect {
                rect: *r,
                brush: Brush::Solid(Color::TRANSPARENT),
                radii: CornerRadii::all(0.0),
                border: Some(Border {
                    width: 1.0,
                    color: *color,
                }),
            });
        }
        let zoom = (self.scale * scale_mul).max(0.1);
        let pw = (self.size.width * zoom).round().max(1.0) as u32;
        let ph = (self.size.height * zoom).round().max(1.0) as u32;
        let bg = Color::srgb8(255, 255, 255, 255);
        let full = self.renderer.render_frame(&dl, pw, ph, zoom, bg);
        let x0 = (region.x0 * zoom).floor().clamp(0.0, pw as f64) as u32;
        let y0 = (region.y0 * zoom).floor().clamp(0.0, ph as f64) as u32;
        let x1 = (region.x1 * zoom).ceil().clamp(0.0, pw as f64) as u32;
        let y1 = (region.y1 * zoom).ceil().clamp(0.0, ph as f64) as u32;
        full.crop(
            x0,
            y0,
            x1.saturating_sub(x0).max(1),
            y1.saturating_sub(y0).max(1),
        )
    }

    /// Wire a live window surface to the renderer for direct present (1c).
    /// Returns whether the backend accepted it (GPU present); on `false` the
    /// shell keeps the CPU readback + separate-presenter path. `width`/`height`
    /// are physical px.
    #[cfg(feature = "wgpu")]
    pub fn attach_surface(
        &mut self,
        target: lumen_render::wgpu::SurfaceTarget<'static>,
        width: u32,
        height: u32,
    ) -> bool {
        let ok = self.renderer.attach_surface(target, width, height);
        self.surface_attached = ok;
        ok
    }

    /// Reconfigure the attached surface to a new physical size (1c).
    #[cfg(feature = "wgpu")]
    pub fn resize_surface(&mut self, width: u32, height: u32) {
        self.renderer.resize_surface(width, height);
    }

    /// Present the most recent frame straight to the attached swapchain (1c) —
    /// no CPU readback. Returns `false` if nothing is attached or there's no
    /// frame yet. The shell calls this after `pump()` when the frame changed.
    #[cfg(feature = "wgpu")]
    pub fn present_to_surface(&mut self) -> bool {
        if !self.surface_attached {
            return false;
        }
        let Some(dl) = self.last_dl.take() else {
            return false;
        };
        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
        let bg = Color::srgb8(255, 255, 255, 255);
        let ok = self
            .renderer
            .present_to_surface(&dl, pw, ph, self.scale, bg);
        self.last_dl = Some(dl);
        ok
    }

    /// Force the next paint to repaint the whole frame instead of only the
    /// damaged region (R2). The shell calls this when the retained frame can't be
    /// trusted — e.g. after the surface is recreated; tests use it to compare the
    /// incremental result against a from-scratch render.
    pub fn force_full_repaint(&mut self) {
        self.last_dl = None;
        self.force_rebuild = true;
        self.pump();
    }

    /// The damage applied by the most recent paint (R2).
    pub fn last_damage(&self) -> Damage {
        self.last_damage
    }

    /// Capture a tier-3 [`AppSnapshot`] (reactive store + focus) for a later
    /// restart via [`App::run_headless_restored`]. Snapshot builds only.
    #[cfg(feature = "snapshot")]
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

    /// The reactive runtime backing this app (state store + scheduler). Lets
    /// tests/tools read `write_gen`/`is_quiescent` and drive signals directly.
    pub fn runtime(&self) -> &Runtime {
        &self.rt
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

    /// The semantics document as JSON (`lumen-semantics/1`, 03 §1). Snapshot
    /// builds only (the agent introspection path).
    #[cfg(feature = "snapshot")]
    pub fn semantics_json(&self) -> serde_json::Value {
        self.semantics_doc().to_json(false)
    }

    /// Structured diagnostics for the current frame (e.g. `W0103` layout
    /// overflow). Lets an agent detect and fix layout bugs by code.
    pub fn diagnostics(&self) -> Vec<lumen_core::Diagnostic> {
        let mut diags = crate::audit::lint(&self.semantics_doc().root);
        if let Some(d) = &self.build_panic {
            diags.push(d.clone());
        }
        diags
    }

    /// The absolute visual-invariant lint (overflow / clipping / zero-area
    /// interactive) over the current tree — see [`audit::lint`](crate::audit::lint).
    /// Unlike goldens, catches first-time layout/render defects; usable in tests
    /// and via the agent (`ui.lint`).
    pub fn lint(&self) -> Vec<lumen_core::Diagnostic> {
        crate::audit::lint(&self.semantics_doc().root)
    }

    // --- desktop system integration (T5.2) ---------------------------------

    /// Read the (in-memory) clipboard text. Backed by the shared `Runtime`
    /// clipboard, so text widgets and this accessor see the same buffer.
    pub fn clipboard_read(&self) -> String {
        self.rt.clipboard()
    }

    /// Write text to the clipboard.
    pub fn clipboard_write(&mut self, text: impl Into<String>) {
        self.rt.set_clipboard(text);
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
        // W.0 (ADR-W1): a custom leaf at the event's target gets first
        // refusal — pointer events at the hit-test target, key/text at the
        // focused node. `Handled` consumes the event: no Element-level
        // handlers, no default routing.
        if let Some(node) = self.leaf_event_target(&ev) {
            if let Some(m) = self.meta.get(&node) {
                if let NodeContent::Custom(w) = &m.content {
                    let w = w.clone();
                    let bounds = self.tree.bounds(node);
                    if matches!(
                        w.event(&ev, bounds, &self.rt),
                        lumen_core::events::EventStatus::Handled
                    ) {
                        return;
                    }
                }
            }
        }
        match ev {
            Event::PointerDown(pe) => {
                // Bubble from the hit target up its ancestors, firing the
                // nearest focus/click/drag handlers (decorative children let
                // their interactive ancestor handle the press).
                let mut n = self.tree.hit_test(pe.pos);
                let (mut did_focus, mut did_click, mut did_drag) = (false, false, false);
                let mut caret_hit = None;
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
                            self.pressed = Some((node, m.id.clone()));
                            self.apply_drag(node, pe.pos);
                            did_drag = true;
                        }
                        // A text editor places its caret at the press and keeps
                        // `pressed` so a drag extends the selection.
                        if caret_hit.is_none() && m.on_caret_set.is_some() {
                            self.pressed = Some((node, m.id.clone()));
                            caret_hit = Some(node);
                        }
                    }
                    if did_focus && did_click && did_drag {
                        break;
                    }
                    let p = self.tree.parent(node);
                    n = p.is_some().then_some(p);
                }
                if let Some(node) = caret_hit {
                    self.place_caret(node, pe.pos, false);
                }
                // Light dismiss: any element with an `on_dismiss` whose bounds do
                // not contain the press is dismissed (click-away for dropdowns/
                // popovers/menus). The opening press never self-dismisses: the
                // overlay is built on the *next* rebuild, so it isn't in this
                // frame's tree yet.
                self.dismiss_outside(pe.pos);
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
                if let Some((idx, drag_id)) = self.pressed.clone() {
                    // Re-resolve by stable id so a rebuild that renumbered nodes
                    // doesn't drag the wrong (or a stale) node; fall back to the
                    // original index.
                    let node = drag_id
                        .as_ref()
                        .and_then(|i| self.node_by_id(i))
                        .unwrap_or(idx);
                    // A pressed text editor extends its selection on drag; other
                    // pressed nodes are sliders/scrollbars (fractional drag).
                    if self
                        .meta
                        .get(&node)
                        .is_some_and(|m| m.on_caret_set.is_some())
                    {
                        self.place_caret(node, pe.pos, true);
                    } else {
                        self.apply_drag(node, pe.pos);
                    }
                }
            }
            Event::Wheel(we) => {
                // Find the nearest ancestor (incl. target) with a wheel handler.
                let mut n = self.tree.hit_test(we.pos);
                while let Some(node) = n {
                    if let Some(h) = self.meta.get(&node).and_then(|m| m.on_wheel.clone()) {
                        h(&self.rt, we.delta.x, we.delta.y, we.modifiers);
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
            Event::KeyDown(ke) => {
                // The focused node's key handler sees every key first (a list
                // handles PageUp/Down/Home/End/arrows); built-in focus/activation
                // keys still apply.
                if let Some(node) = self.focused_node() {
                    // Vertical caret nav needs layout geometry (which visual line),
                    // so the app handles Up/Down for text editors; the widget's
                    // on_key handles the rest (Left/Right/Home/End/edit/clipboard).
                    let vnav = match ke.key {
                        Key::Named(NamedKey::ArrowUp) => Some(true),
                        Key::Named(NamedKey::ArrowDown) => Some(false),
                        _ => None,
                    }
                    .filter(|_| {
                        self.meta
                            .get(&node)
                            .is_some_and(|m| m.on_caret_set.is_some())
                    });
                    if let Some(up) = vnav {
                        let extend = ke.modifiers.contains(lumen_core::events::Modifiers::SHIFT);
                        self.move_caret_vertical(node, up, extend);
                    } else if let Some(h) = self.meta.get(&node).and_then(|m| m.on_key.clone()) {
                        h(&self.rt, &ke);
                    }
                }
                match ke.key {
                    Key::Named(NamedKey::Tab) => {
                        let forward = !ke.modifiers.contains(lumen_core::events::Modifiers::SHIFT);
                        self.move_focus(forward);
                    }
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.activate_focused();
                    }
                    // Escape light-dismisses every open overlay.
                    Key::Named(NamedKey::Escape) => self.dismiss_all(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Fire `on_dismiss` for every element whose bounds do not contain `pos`
    /// (click-away). Collected first, since a handler mutates state.
    fn dismiss_outside(&self, pos: Point) {
        let hits: Vec<Handler> = self
            .tree
            .document_order()
            .into_iter()
            .filter_map(|n| {
                let h = self.meta.get(&n).and_then(|m| m.on_dismiss.clone())?;
                (!self.tree.bounds(n).contains(pos)).then_some(h)
            })
            .collect();
        for h in hits {
            h(&self.rt);
        }
    }

    /// Fire every `on_dismiss` (Escape closes all overlays).
    fn dismiss_all(&self) {
        let hits: Vec<Handler> = self
            .tree
            .document_order()
            .into_iter()
            .filter_map(|n| self.meta.get(&n).and_then(|m| m.on_dismiss.clone()))
            .collect();
        for h in hits {
            h(&self.rt);
        }
    }

    fn focused_node(&self) -> Option<NodeIndex> {
        let id = self.focused_id.as_ref()?;
        self.node_by_id(id)
    }

    /// The current node carrying stable id `id`, if any (survives rebuilds).
    fn node_by_id(&self, id: &StableId) -> Option<NodeIndex> {
        self.tree
            .document_order()
            .into_iter()
            .find(|n| self.meta.get(n).and_then(|m| m.id.as_ref()) == Some(id))
    }

    /// The rendered bounds of the node with stable id `id`, if present. Looked
    /// up by id (not node index), so it survives the rebuilds that renumber
    /// nodes — handy for asserting a layout reflowed after a state change.
    pub fn node_bounds_by_id(&self, id: &str) -> Option<Rect> {
        let id: StableId = id.into();
        self.node_by_id(&id).map(|n| self.tree.bounds(n))
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

    /// Call a node's drag handler with the pointer's fraction along its width and
    /// height (`frac_x`, `frac_y`). Horizontal controls read `frac_x`, vertical
    /// ones (a scrollbar) read `frac_y`.
    fn apply_drag(&self, node: NodeIndex, pos: Point) {
        let b = self.tree.bounds(node);
        if b.width() <= 0.0 && b.height() <= 0.0 {
            return; // degenerate/stale bounds — skip rather than apply (0, 0)
        }
        let frac_x = if b.width() > 0.0 {
            ((pos.x - b.x0) / b.width()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let frac_y = if b.height() > 0.0 {
            ((pos.y - b.y0) / b.height()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Some(h) = self.meta.get(&node).and_then(|m| m.on_drag.clone()) {
            h(&self.rt, frac_x, frac_y, pos);
        }
    }

    /// Resolve a pointer position over a text-editor node to a byte offset (via
    /// the text engine's geometry) and call its caret handler. `extend` keeps the
    /// selection anchor (drag-select). No-op for non-editor nodes.
    fn place_caret(&mut self, node: NodeIndex, pos: Point, extend: bool) {
        let b = self.tree.bounds(node);
        let Some((text, ts, wrap, padx, pady, handler)) = self.meta.get(&node).and_then(|m| {
            let h = m.on_caret_set.clone()?;
            let NodeContent::Text(t, ts) = &m.content else {
                return None;
            };
            Some((t.clone(), ts.clone(), m.wrap_width, m.pad.0, m.pad.1, h))
        }) else {
            return;
        };
        // Content-box-local px: x=0 is before the first glyph (matches the text
        // origin, which is painted at the padded corner).
        let lx = (pos.x - b.x0 - padx) as f32;
        let ly = (pos.y - b.y0 - pady) as f32;
        let block = self
            .text
            .layout(&text, ts, &[], wrap, lumen_text::TextAlign::Start);
        let byte = block.hit_to_byte(lx, ly);
        handler(&self.rt, byte, extend);
    }

    /// Move a text-editor caret up/down a visual line (geometry lives here, on the
    /// engine side). Resolves the current caret's x to the line above/below and
    /// calls the caret handler. `extend` keeps the selection anchor (Shift).
    fn move_caret_vertical(&mut self, node: NodeIndex, up: bool, extend: bool) {
        let Some((text, ts, wrap, caret, handler)) = self.meta.get(&node).and_then(|m| {
            let h = m.on_caret_set.clone()?;
            let c = m.caret_byte?;
            let NodeContent::Text(t, ts) = &m.content else {
                return None;
            };
            Some((t.clone(), ts.clone(), m.wrap_width, c, h))
        }) else {
            return;
        };
        let block = self
            .text
            .layout(&text, ts, &[], wrap, lumen_text::TextAlign::Start);
        let (x, y, h) = block.caret_pos(caret);
        // Probe into the neighbouring line (above the caret top, or below its
        // baseline); hit_to_byte clamps to the nearest cluster on that line.
        let ty = if up { y - h * 0.5 } else { y + h * 1.5 };
        let byte = block.hit_to_byte(x, ty);
        handler(&self.rt, byte, extend);
    }

    // --- rebuild ------------------------------------------------------------

    /// Rebuild, containing any panic in the build/layout/paint so a buggy frame
    /// can't take down the window (C2 / T7.3). On panic the previous good frame
    /// is kept and a structured `E0701` diagnostic is recorded; a clean build
    /// clears it.
    /// Rebuild the whole view from current state, bypassing every incremental
    /// cache — the **coherence oracle** (F0): the tree as a pure function of the
    /// store. Snapshot/restore, the CPU golden, replay determinism, and hot
    /// reload all reduce to this one operation, and the fine-grained work (F1+)
    /// must stay equal to it (`assert_view_coherent`).
    pub fn rebuild_fresh(&mut self) {
        self.clear_view_caches();
        self.rebuild();
    }

    /// Drop all memoized `cx.scope` subtrees so the next build is from scratch.
    /// Centralised here so the oracle (`rebuild_fresh`), hot reload, and
    /// non-signal rebuilds (resize/theme/visual-state) share one invalidation
    /// point — those paths change the frame without a tracked signal write, so
    /// the version-based memo can't see them.
    fn clear_view_caches(&mut self) {
        self.scope_cache.borrow_mut().clear();
    }

    /// F5 GC: drop cached scope subtrees + scope-local signals whose key was not
    /// accessed this build (a keyed-list item that vanished). Keeps a churning
    /// list bounded; correct because an absent scope isn't in the view, so a
    /// fresh rebuild wouldn't produce it either (coherence preserved).
    fn sweep_dead_scopes(&mut self) {
        let dead: Vec<String> = {
            let live = self.scope_live.borrow();
            let cache = self.scope_cache.borrow();
            cache
                .keys()
                .filter(|k| !live.contains(*k))
                .cloned()
                .collect()
        };
        for k in dead {
            self.scope_cache.borrow_mut().remove(&k);
            self.rt.evict_prefix(&format!("{k}/"));
        }
    }

    /// Assert the current (possibly incrementally-updated) view equals a fresh
    /// rebuild from the same state — the F0 coherence invariant
    /// `incremental == rebuild_fresh`. Compares the display list (render truth,
    /// `DrawCmd: PartialEq`) and the semantics tree (agent truth, via `Debug`).
    /// Trivially true today (every pump is already a fresh rebuild); it gains
    /// teeth as F1/F2 add memoized/retained subtrees. Intended for tests + CI
    /// over the gallery and examples.
    pub fn assert_view_coherent(&mut self) {
        let dl_before = self.last_dl.as_ref().map(|d| d.cmds.clone());
        let sem_before = self.sem_root.as_ref().map(|s| format!("{s:?}"));
        self.rebuild_fresh();
        let dl_after = self.last_dl.as_ref().map(|d| d.cmds.clone());
        let sem_after = self.sem_root.as_ref().map(|s| format!("{s:?}"));
        assert!(
            dl_before == dl_after,
            "view incoherent: display list differs from a fresh rebuild"
        );
        assert!(
            sem_before == sem_after,
            "view incoherent: semantics tree differs from a fresh rebuild"
        );
    }

    /// F3.4: re-evaluate the paint-only bindings whose deps changed, patch each
    /// node's background in the retained `meta`, and repaint. R2 damage limits
    /// the raster to exactly the changed region — no rebuild, no relayout, no
    /// scope re-run. The retained tree stays a pure function of the store
    /// (guarded by `assert_view_coherent`).
    fn patch_bg_bindings(&mut self) {
        let rt = self.rt.clone();
        let stale: Vec<usize> = self
            .bg_bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.deps.is_current(&rt))
            .map(|(i, _)| i)
            .collect();
        let mut patched: Vec<u32> = Vec::new();
        for i in stale {
            let (color, reads) = self.bg_bindings[i].dynamic.eval_isolated(&rt);
            let node = self.bg_bindings[i].node;
            self.bg_bindings[i].deps = reads;
            if let Some(m) = self.meta.get_mut(&node) {
                m.background = Some(color);
            }
            patched.push(node.index());
        }
        self.last_damage = self.paint();
        self.last_build_gen = self.rt.write_gen();
        self.last_change = ChangeReport {
            kind: "patch",
            nodes: patched,
        };
    }

    fn rebuild(&mut self) {
        // Default to "nothing painted"; a successful paint sets the real damage.
        self.last_damage = Damage::None;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.rebuild_inner()));
        match result {
            Ok(()) => self.build_panic = None,
            Err(payload) => {
                let msg = panic_msg(&payload);
                // C.2: panics reach the agent's `app.logs` too, not just
                // `app.diagnostics` — logs survive after the diagnostic clears.
                self.rt.log("error", format!("E0701 build panicked: {msg}"));
                self.build_panic = Some(lumen_core::Diagnostic::new(
                    lumen_core::codes::E0701,
                    format!("build panicked (frame contained): {msg}"),
                ));
            }
        }
        // Baseline the skip-rebuild state after every build, so the next pump only
        // rebuilds on a real change (the build itself may bump the write-gen via
        // memo recomputes — capture the post-build value).
        self.force_rebuild = false;
        self.last_build_gen = self.rt.write_gen();
        self.last_build_clock = self.clock_ms;
        // F4.3: a structural rebuild. Per-node change-diffing is deferred; the
        // agent reads the fresh tree via `getTree`. (Patches report exact nodes.)
        self.last_change = ChangeReport {
            kind: "rebuild",
            nodes: Vec::new(),
        };
    }

    fn rebuild_inner(&mut self) {
        // F3.4: capture the root build's reads (structural — a change rebuilds).
        // Scope reads propagate into this window; paint-only bindings evaluated
        // in `build_node` isolate themselves out; text-binding reads are folded
        // into `structural_reads` there.
        let rt = self.rt.clone();
        self.scope_live.borrow_mut().clear();
        let (root_el, requests, root_reads) = {
            let mut cx = BuildCx::new(
                &self.rt,
                self.clock_ms,
                &self.scope_cache,
                &self.scope_live,
                self.size,
            );
            let (el, reads) = rt.collect_reads(|| (self.root)(&mut cx));
            (el, cx.take_requests(), reads)
        };
        // F5 GC: sweep cached scopes + scope-local signals absent this build.
        self.sweep_dead_scopes();
        self.requests = requests;
        self.structural_reads = root_reads;
        self.bg_bindings.clear();

        // Dispatch background-work requests this build emitted, on the executor.
        // The runtime owns the executor + the deferred-op channel, so it mints
        // the sink here (the executor never leaked into `BuildCx`). Results flow
        // back through the channel and are applied at the top of the next pump.
        let tasks = std::mem::take(&mut self.requests.tasks);
        for req in tasks {
            let sink = self.rt.make_sink_with(self.task_waker.clone());
            match req {
                crate::element::TaskRequest::Blocking(job) => {
                    self.executor.spawn_blocking(Box::new(move || job(sink)));
                }
                crate::element::TaskRequest::Future(make) => {
                    self.executor.spawn(make(sink));
                }
            }
        }

        // A.2: styles resolve *before* layout, inline in `build_node`, so
        // `.lss` layout properties reach taffy. Build the cascade env once
        // per rebuild; clear the per-node results (NodeIndex values are
        // generational — a reused index must not inherit a stale style).
        self.node_style.clear();
        self.node_computed.clear();
        self.scope_spans.clear();
        self.desc_stack.clear();
        self.container_nodes.clear();
        self.container_stack.clear();
        self.style_env = self.app_sheet.as_ref().map(|sheet| StyleEnv {
            sources: [lumen_style::StyleSource {
                origin: lumen_style::Origin::App,
                sheet: sheet.clone(),
            }],
            tokens: lumen_style::tokens_for(sheet, self.theme),
            media: lumen_style::MediaContext {
                width: self.size.width,
                height: self.size.height,
                scale: self.scale,
                platform: if cfg!(target_os = "windows") {
                    "windows"
                } else if cfg!(target_os = "macos") {
                    "macos"
                } else if cfg!(target_os = "android") {
                    "android"
                } else if cfg!(target_os = "ios") {
                    "ios"
                } else {
                    "linux"
                }
                .to_string(),
                // Desktop shells synthesize mouse pointers; the mobile
                // shells flip this to "touch" when they wire input (P.1).
                pointer: if cfg!(any(target_os = "android", target_os = "ios")) {
                    "touch"
                } else {
                    "mouse"
                }
                .to_string(),
                // B.2b: per-node — set from `container_stack` at resolve time.
                container: None,
            },
        });

        let mut tree = Tree::new();
        let mut layout = LayoutTree::new();
        let mut meta = HashMap::new();
        let mut built: Vec<(NodeIndex, LayoutNode)> = Vec::new();
        let (_root_node, root_lnode) = self.build_node(
            root_el,
            &mut tree,
            &mut layout,
            &mut meta,
            &mut built,
            None,
            false,
        );

        layout.compute(root_lnode, self.size);
        if self.rtl {
            layout.mirror_rtl(root_lnode);
        }
        for (node, lnode) in &built {
            tree.set_bounds(*node, layout.bounds(*lnode));
        }

        // B.2b: container queries resolved against the *previous* layout's
        // container sizes; if this layout measured them differently, one
        // bounded re-pass lets queries see the fresh sizes within this pump
        // (a change caused *by* the re-pass itself waits for the next one —
        // prevents oscillation).
        let sizes: Vec<(f64, f64)> = self
            .container_nodes
            .iter()
            .map(|n| {
                let b = tree.bounds(*n);
                (b.width(), b.height())
            })
            .collect();
        if sizes != self.container_prev {
            self.container_prev = sizes;
            if !self.container_repass {
                self.container_repass = true;
                self.rebuild_inner();
                self.container_repass = false;
                return;
            }
        }

        self.tree = tree;
        self.meta = meta;
        self.last_damage = self.paint();
        self.sem_root = Some(self.build_semantics(self.tree.root()));
        self.rebuild_dep_index();
    }

    /// F4.2: rebuild the reverse index (signal key → dependent nodes) from the
    /// per-node `NodeDeps`. `background` deps update via a patch, `scope`/`text`
    /// via a rebuild.
    fn rebuild_dep_index(&mut self) {
        let mut idx: HashMap<String, Vec<DepEntry>> = HashMap::new();
        for (node, m) in &self.meta {
            let id = node.index();
            let mut add = |keys: &[String], via, update| {
                for k in keys {
                    idx.entry(k.clone()).or_default().push(DepEntry {
                        node: id,
                        via,
                        update,
                    });
                }
            };
            add(&m.deps.scope, "scope", "rebuild");
            add(&m.deps.text, "text", "rebuild");
            add(&m.deps.background, "background", "patch");
            add(&m.deps.class, "class", "rebuild");
        }
        self.dep_index = idx;
    }

    /// The node span a [`BuildCx::scope`](crate::BuildCx::scope) produced this
    /// build: its subtree-root node and preorder node count (A.3.1). `key` is
    /// the full scope key (the `id` passed to `scope`, prefixed by enclosing
    /// scopes). Introspection for the retained-pipeline work and tests.
    pub fn scope_span(&self, key: &str) -> Option<(NodeIndex, u32)> {
        self.scope_spans.get(key).copied()
    }

    /// Set/replace the app stylesheet at runtime (tier-1 hot reload). A broken
    /// edit is rejected and the previous stylesheet stays live (04 §9).
    pub fn set_stylesheet(&mut self, src: &str) -> ReloadResult {
        let (sheet, diags) = lumen_style::parse("app.lss", src);
        if lumen_style::has_errors(&diags) {
            // C.2: reload rejections reach `app.logs` (the previous sheet
            // stays live, so the only other trace is stderr).
            self.rt.log(
                "warn",
                format!("stylesheet rejected ({} diagnostics)", diags.len()),
            );
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
    /// to exactly one node. Snapshot builds only (the agent introspection path).
    #[cfg(feature = "snapshot")]
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
                map.insert(
                    prop.clone(),
                    lumen_style::computed_json_spanned(&c.value, c.origin, c.span),
                );
            }
        }
        serde_json::Value::Object(map)
    }

    /// The reactive dependencies of the node a `selector` resolves to (F4
    /// `ui.getDeps`): the union of signal keys plus a per-prop breakdown
    /// (`scope`, `text`, `background`). `null` if the selector doesn't resolve to
    /// exactly one node. Snapshot builds only.
    #[cfg(feature = "snapshot")]
    pub fn get_deps(&self, selector: &str) -> serde_json::Value {
        let root = self.semantics_doc().root.elided();
        let Ok(id) = lumen_core::semantics::resolve_one(&root, selector) else {
            return serde_json::Value::Null;
        };
        let node = self
            .tree
            .document_order()
            .into_iter()
            .find(|n| n.index() == id);
        let Some(deps) = node.and_then(|n| self.meta.get(&n)).map(|m| &m.deps) else {
            return serde_json::Value::Null;
        };
        serde_json::json!({
            "node": format!("node-{id}"),
            "deps": deps.union(),
            "byProp": {
                "scope": deps.scope,
                "text": deps.text,
                "background": deps.background,
                "class": deps.class,
            },
        })
    }

    /// The nodes that depend on `signal` and how they'd update if it changed
    /// (F4.2 `ui.whatDependsOn`) — predictive, no write. Empty for a signal the
    /// view doesn't read. Snapshot builds only.
    #[cfg(feature = "snapshot")]
    pub fn what_depends_on(&self, signal: &str) -> serde_json::Value {
        let dependents: Vec<serde_json::Value> = self
            .dep_index
            .get(signal)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|e| {
                serde_json::json!({
                    "node": format!("node-{}", e.node),
                    "via": e.via,
                    "update": e.update,
                })
            })
            .collect();
        serde_json::json!({ "signal": signal, "dependents": dependents })
    }

    /// What the last `pump` did (F4.3 `ui.lastChange`): `kind` is
    /// `idle`/`patch`/`rebuild`; `nodes` are the exact patched nodes (a rebuild
    /// reports none — read the fresh tree via `getTree`). Snapshot builds only.
    #[cfg(feature = "snapshot")]
    pub fn last_change(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.last_change.kind,
            "nodes": self.last_change.nodes.iter().map(|n| format!("node-{n}")).collect::<Vec<_>>(),
        })
    }

    /// Activate a control by running its retained handler directly (F4.4),
    /// instead of synthesizing a pointer at its centre and re-hit-testing — more
    /// robust under overlap/transforms. `action` is `click`/`focus`/`dismiss`.
    /// Pumps afterward; returns the node index or an error string.
    pub fn invoke_action(&mut self, selector: &str, action: &str) -> Result<u32, String> {
        let root = self.semantics_doc().root.elided();
        let id = lumen_core::semantics::resolve_one(&root, selector)
            .map_err(|_| format!("selector `{selector}` did not resolve to one node"))?;
        let node = self
            .tree
            .document_order()
            .into_iter()
            .find(|n| n.index() == id)
            .ok_or_else(|| "resolved node is not live".to_string())?;
        let m = self.meta.get(&node);
        match action {
            "click" => {
                let handler = m.and_then(|m| m.on_click.clone());
                match handler {
                    Some(h) => h(&self.rt),
                    None => return Err(format!("node `{selector}` has no click handler")),
                }
            }
            "focus" => self.focused_id = m.and_then(|m| m.id.clone()),
            "dismiss" => {
                let handler = m.and_then(|m| m.on_dismiss.clone());
                if let Some(h) = handler {
                    h(&self.rt);
                }
            }
            other => return Err(format!("unsupported action `{other}`")),
        }
        self.pump();
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_node(
        &mut self,
        mut el: Element,
        tree: &mut Tree,
        layout: &mut LayoutTree,
        meta: &mut HashMap<NodeIndex, NodeMeta>,
        built: &mut Vec<(NodeIndex, LayoutNode)>,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        // A.3.1: a scope-root element records its node span. Nodes allocate
        // preorder in the fresh per-rebuild tree, so a subtree is the
        // contiguous range [span_start, tree.len()) once its children are
        // lowered — the anchor the retained-graph splice (A.3.3) replaces.
        // Taken before the children are consumed (partial-move below).
        let span_start = tree.len();
        let span_key = el.scope_key.take();
        let node = match parent {
            None => tree.insert_root(),
            Some(p) => tree.insert_child(p),
        };
        // Overlay subtrees (dropdown menus, popovers, tooltips) paint in a final
        // top pass that escapes ancestor clips. Hit-testing keys on `z` first, so
        // give them an elevated z to match — otherwise content that paints *under*
        // the overlay but comes later in document order would steal its clicks.
        let this_overlay = in_overlay || el.overlay;

        // F3: evaluate reactive prop bindings *before* the content is read for
        // hit-testing/measurement, recording their dependency keys per prop (F4).
        let mut text_deps: Vec<String> = Vec::new();
        let mut bg_deps: Vec<String> = Vec::new();
        let mut class_deps: Vec<String> = Vec::new();
        if el.dyn_text.is_some() || el.dyn_bg.is_some() || el.dyn_classes.is_some() {
            let rt = self.rt.clone();
            if let Some(d) = el.dyn_classes.clone() {
                // Classes drive the `.lss` cascade (may change size) → NON-isolated
                // (structural). Appended to the static classes.
                let (classes, reads) = d.eval(&rt);
                class_deps = reads.dep_keys(&rt);
                self.structural_reads.extend(&reads);
                el.classes.extend(classes);
            }
            if let Some(d) = el.dyn_text.clone() {
                // Text is size-affecting → NON-isolated: its reads are structural
                // (a change relayouts via a full rebuild).
                let (s, reads) = d.eval(&rt);
                text_deps = reads.dep_keys(&rt);
                self.structural_reads.extend(&reads);
                // The string is the node's content *and* its accessible label
                // (Element::text sets both); keep them in sync.
                el.label = s.clone();
                el.content = match std::mem::take(&mut el.content) {
                    NodeContent::Text(_, ts) => NodeContent::Text(s, ts),
                    _ => NodeContent::Text(s, lumen_text::TextStyle::default()),
                };
            }
            if let Some(d) = el.dyn_bg.clone() {
                // Background is paint-only → ISOLATED + retained: a change patches
                // this node in place without a rebuild (F3.4).
                let (c, reads) = d.eval_isolated(&rt);
                bg_deps = reads.dep_keys(&rt);
                el.background = Some(c);
                self.bg_bindings.push(BoundBg {
                    node,
                    dynamic: d,
                    deps: reads,
                });
            }
        }
        let node_deps = NodeDeps {
            scope: el.scope_deps.take().unwrap_or_default(),
            text: text_deps,
            background: bg_deps,
            class: class_deps,
        };

        let mut flags = NodeFlags::VISIBLE;
        let interactive = el.background.is_some()
            || el.on_click.is_some()
            || matches!(
                el.content,
                NodeContent::Text(..) | NodeContent::Image(..) | NodeContent::Custom(..)
            )
            || el.on_wheel.is_some()
            || el.on_drag.is_some()
            || el.on_key.is_some()
            || el.focusable;
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
        if this_overlay {
            tree.set_z(node, OVERLAY_Z);
        }

        // A.2: resolve this node's `.lss` rules *now*, before anything
        // consumes `el.style` (text wrap, custom measure, taffy), so layout
        // properties from the stylesheet are real. Per-node resolution needs
        // no ancestry (compound selectors), dynamic classes were merged into
        // `el.classes` above, and the visual states are known from the flags
        // just computed. Paint properties land in `node_style`/
        // `node_computed` exactly as the old post-layout pass produced them
        // (`emit_pass`/`get_styles` are unchanged consumers).
        //
        // NOTE for A.3.2 (retained scopes): this mutates the *owned* element;
        // once memo hits become shared `Rc` subtrees the merge must move to a
        // per-node copy instead.
        if let Some(env) = &self.style_env {
            // B.6a: the full state vocabulary — interaction states carry
            // their CSS-familiar aliases (spec examples write `:hover`), and
            // the widget's semantic states (checked/disabled/expanded/…)
            // are style-matchable, so `checkbox:checked { … }` just works.
            let mut states = Vec::new();
            if flags.contains(NodeFlags::FOCUSED) {
                states.push("focused".to_string());
                states.push("focus".to_string());
            }
            if flags.contains(NodeFlags::HOVERED) {
                states.push("hovered".to_string());
                states.push("hover".to_string());
            }
            if el.id.is_some() && self.pressed.as_ref().is_some_and(|(_, id)| *id == el.id) {
                states.push("pressed".to_string());
                states.push("active".to_string());
            }
            states.extend(el.states.iter().map(|s| s.as_str().to_string()));
            let desc = lumen_style::NodeDesc {
                id: el.id.as_ref().map(|i| i.as_str().to_string()),
                classes: el.classes.clone(),
                states,
                ty: el.role.as_str().to_string(),
            };
            // B.1: the recursion's ancestor chain makes descendant/`>`
            // selectors real (previously only the rightmost compound was
            // checked — `dialog button` matched every button). B.2: the live
            // media context gates `@media` blocks on the actual window.
            // B.2b: inside a `.container()`, container queries test that
            // ancestor's size (from the last layout) instead of the window.
            let media = match self.container_stack.last().copied().flatten() {
                Some(size) => std::borrow::Cow::Owned(lumen_style::MediaContext {
                    container: Some(size),
                    ..env.media.clone()
                }),
                None => std::borrow::Cow::Borrowed(&env.media),
            };
            let computed =
                lumen_style::resolve_with_ancestors(&env.sources, &desc, &self.desc_stack, &media);
            let mut css = lumen_style::Style::new();
            let mut resolved = HashMap::new();
            for (prop, c) in &computed {
                lumen_style::apply(&mut css, prop, &c.value, &env.tokens);
                // Store the token-resolved value so `get_styles` returns the
                // computed (substituted) form (04 §7).
                resolved.insert(
                    prop.clone(),
                    lumen_style::Computed {
                        value: lumen_style::resolve_token(&c.value, &env.tokens),
                        important: c.important,
                        origin: c.origin,
                        span: c.span,
                    },
                );
            }
            // Layout overrides: only the fields the sheet actually set win
            // over the element's `LayoutStyle` (element < .lss, matching the
            // cascade's origin order once B.6 adds more origins).
            if let Some(d) = css.display {
                el.style.display = d;
            }
            if let Some(f) = css.flex_direction {
                el.style.flex_direction = f;
            }
            if let Some(w) = css.width {
                el.style.width = w;
            }
            if let Some(h) = css.height {
                el.style.height = h;
            }
            if let Some(g) = css.gap {
                el.style.row_gap = g;
                el.style.column_gap = g;
            }
            if let Some(p) = css.padding {
                el.style.padding = p;
            }
            if let Some(m) = css.margin {
                el.style.margin = m;
            }
            // B.4: typography reaches the text stack — the measured and the
            // painted TextStyle are the same object (content moves into
            // NodeMeta), so one override covers both passes.
            if css.font_size.is_some() || css.font_weight.is_some() || css.line_height.is_some() {
                if let NodeContent::Text(_, ts) = &mut el.content {
                    if let Some(fs) = css.font_size {
                        ts.font_size = fs;
                    }
                    if let Some(w) = css.font_weight {
                        ts.weight = w as f32;
                    }
                    if let Some(lh) = css.line_height {
                        ts.line_height = Some(lh);
                    }
                }
            }
            self.node_style.insert(node, css);
            self.node_computed.insert(node, resolved);
            // B.1: this node becomes an ancestor for its children's matching
            // (popped after the recursion below).
            self.desc_stack.push(desc);
        }
        let pushed_desc = self.style_env.is_some();
        // B.2b: this node's own styles resolved against the *enclosing*
        // container (CSS semantics); its descendants query this one. Size
        // comes from the previous layout by build order (`None` until
        // measured — queries fail closed for that pass).
        let pushed_container = el.container;
        if pushed_container {
            let seq = self.container_nodes.len();
            self.container_nodes.push(node);
            self.container_stack
                .push(self.container_prev.get(seq).copied());
        }

        // Text nodes get a fixed size from measurement.
        let mut style = el.style;
        let (pl, pt) = (dim_px(style.padding.left), dim_px(style.padding.top));
        let (pr, pb) = (dim_px(style.padding.right), dim_px(style.padding.bottom));
        let pad = (pl, pt);
        let mut text_wrap: Option<f32> = None;
        if let NodeContent::Text(txt, ts) = &el.content {
            // An explicit pixel width turns the label into a wrapping paragraph:
            // we lay out into the content box (width minus horizontal padding) and
            // keep that width, taking only the (wrapped) height from the block.
            // Otherwise the box is sized to the unwrapped text *plus* padding so
            // the label has room; it's then painted at the padded origin.
            let wrap = match style.width {
                Dim::Px(w) => Some((w - (pl + pr) as f32).max(0.0)),
                _ => None,
            };
            let block = self
                .text
                .shaped(txt, ts, wrap, lumen_text::TextAlign::Start);
            if wrap.is_none() {
                style.width = Dim::px(block.width().ceil() + (pl + pr) as f32);
            }
            style.height = Dim::px(block.height().ceil() + (pt + pb) as f32);
            text_wrap = wrap;
        } else if let NodeContent::Custom(w) = &el.content {
            // Size a custom leaf from its intrinsic measure (E2), but let an
            // explicit `width`/`height` win so a leaf can flex/fill (e.g. a chart
            // at `width: 100%`). The measure sees the constrained axes as available
            // space; only an `Auto` axis is replaced by the intrinsic result.
            let avail = kurbo::Size::new(
                match style.width {
                    Dim::Px(v) => v as f64,
                    _ => f64::INFINITY,
                },
                match style.height {
                    Dim::Px(v) => v as f64,
                    _ => f64::INFINITY,
                },
            );
            let s = w.measure(avail);
            if matches!(style.width, Dim::Auto) {
                style.width = Dim::px(s.width.max(0.0) as f32);
            }
            if matches!(style.height, Dim::Auto) {
                style.height = Dim::px(s.height.max(0.0) as f32);
            }
        }

        // Consume the children (move, not clone) and recurse.
        let child_built: Vec<(NodeIndex, LayoutNode)> = el
            .children
            .into_iter()
            .map(|c| self.build_node(c, tree, layout, meta, built, Some(node), this_overlay))
            .collect();
        if pushed_desc {
            self.desc_stack.pop();
        }
        if pushed_container {
            self.container_stack.pop();
        }
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
                deps: node_deps,
                on_click: el.on_click,
                on_wheel: el.on_wheel,
                on_drag: el.on_drag,
                on_drop: el.on_drop,
                on_text: el.on_text,
                on_key: el.on_key,
                on_caret_set: el.on_caret_set,
                caret_byte: el.caret_byte,
                selection: el.selection,
                on_dismiss: el.on_dismiss,
                background: el.background,
                border: el.border,
                corner_radius: el.corner_radius,
                clip: el.clip,
                overlay: el.overlay,
                shadow: el.shadow,
                content: el.content,
                pad,
                wrap_width: text_wrap,
            },
        );
        built.push((node, lnode));
        if let Some(key) = span_key {
            self.scope_spans
                .insert(key, (node, (tree.len() - span_start) as u32));
        }
        (node, lnode)
    }

    // --- paint --------------------------------------------------------------

    fn build_display_list(&mut self) -> (DisplayList, Vec<lumen_render::TextTarget>) {
        let mut dl = DisplayList::new();
        let mut text_targets: Vec<lumen_render::TextTarget> = Vec::new();
        self.node_ink.clear(); // repopulated per node as text runs are emitted
        self.node_text_metrics.clear();
        let order = self.tree.document_order();
        // Preorder depth of every node, and a partition into the main pass and the
        // overlay pass (nodes inside an `overlay` subtree). Overlays paint last so
        // they sit above the rest of the UI and escape ancestor clips (dropdown
        // menus, popovers, tooltips). Both subsets keep document order.
        let root = order.first().copied();
        let mut depth: HashMap<NodeIndex, u32> = HashMap::new();
        let mut main_order: Vec<NodeIndex> = Vec::new();
        let mut overlay_order: Vec<NodeIndex> = Vec::new();
        let mut overlay_depths: Vec<u32> = Vec::new();
        for node in order {
            let d = if Some(node) == root {
                0
            } else {
                depth.get(&self.tree.parent(node)).map_or(0, |p| p + 1)
            };
            depth.insert(node, d);
            while overlay_depths.last().is_some_and(|&od| d <= od) {
                overlay_depths.pop();
            }
            let is_root = self.meta.get(&node).is_some_and(|m| m.overlay);
            let inside = !overlay_depths.is_empty() || is_root;
            if is_root {
                overlay_depths.push(d);
            }
            if inside {
                overlay_order.push(node);
            } else {
                main_order.push(node);
            }
        }
        self.emit_pass(&main_order, &depth, &mut dl, &mut text_targets);
        self.emit_pass(&overlay_order, &depth, &mut dl, &mut text_targets);
        (dl, text_targets)
    }

    /// Emit draw commands for `order` (a document-ordered node subset), opening/
    /// closing `overflow:hidden` clip layers via a depth-keyed stack.
    fn emit_pass(
        &mut self,
        order: &[NodeIndex],
        depth: &HashMap<NodeIndex, u32>,
        dl: &mut DisplayList,
        text_targets: &mut Vec<lumen_render::TextTarget>,
    ) {
        let mut clip_stack: Vec<u32> = Vec::new();
        for &node in order {
            let bounds = self.tree.bounds(node);
            let d = depth.get(&node).copied().unwrap_or(0);
            while clip_stack.last().is_some_and(|&cd| d <= cd) {
                dl.push(DrawCmd::PopLayer);
                clip_stack.pop();
            }
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
            // B.3: `.lss` opacity < 1 wraps the node's subtree in a
            // compositing layer — tracked on the same depth-keyed stack as
            // the clip layer, so it pops when the subtree ends.
            let opacity = css.and_then(|s| s.opacity).unwrap_or(1.0);
            if opacity < 1.0 {
                dl.push(DrawCmd::PushLayer {
                    clip: None,
                    opacity: opacity.clamp(0.0, 1.0),
                    transform: kurbo::Affine::IDENTITY,
                    blend: BlendMode::SourceOver,
                });
                clip_stack.push(d);
            }
            // overflow:hidden — open a clip layer for this node's subtree (its own
            // fill + descendants paint into it, masked to its rounded bounds).
            if m.clip {
                dl.push(DrawCmd::PushLayer {
                    clip: Some(RoundedRect {
                        rect: bounds,
                        radii: CornerRadii::all(radius),
                    }),
                    opacity: 1.0,
                    transform: kurbo::Affine::IDENTITY,
                    blend: BlendMode::SourceOver,
                });
                clip_stack.push(d);
            }
            // Drop shadow: a soft penumbra — the shadow shape rasterized once and
            // Gaussian-blurred (the shared blur primitive). The sprite is static
            // for a given box, so cache it and blit each frame rather than
            // re-blurring (a large per-frame blur would dominate frame time).
            // B.3: `.lss` shadow overrides the widget's hardcoded one, like
            // background/radius above.
            let shadow = css
                .and_then(|s| s.shadow)
                .map(|ss| crate::element::Shadow {
                    dx: ss.dx as f64,
                    dy: ss.dy as f64,
                    blur: ss.blur as f64,
                    spread: ss.spread as f64,
                    color: ss.color,
                })
                .or(m.shadow);
            if let Some(sh) = shadow {
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
                    // Rasterize the solid shadow shape, then blur it into a soft
                    // penumbra. The margin reserves room for the blur to spread.
                    sdl.push(DrawCmd::Rect {
                        rect: base,
                        brush: Brush::Solid(Color::srgb8(r, g, b, a)),
                        radii: CornerRadii::all((radius + sh.spread).max(0.0)),
                        border: None,
                    });
                    let solid = cpu::render(&sdl, sw.max(1), sh_px.max(1), Color::TRANSPARENT);
                    let img = solid.blurred(sh.blur.round().max(0.0) as u32);
                    const CAP: usize = 64;
                    if self.shadow_cache.len() >= CAP {
                        // R.5: half-retention — sprites are expensive blurs.
                        let mut keep = self.shadow_cache.len() / 2;
                        self.shadow_cache.retain(|_, _| {
                            let k = keep > 0;
                            keep = keep.saturating_sub(1);
                            k
                        });
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
            // Glass: blur the painted backdrop within this node's box before its
            // (translucent) fill goes on top. Emitted after the shadow so it
            // filters everything behind, but before bg/children.
            let blur = css.and_then(|s| s.backdrop_blur).unwrap_or(0.0);
            let refraction = css.and_then(|s| s.backdrop_refraction).unwrap_or(0.0);
            let specular = css.and_then(|s| s.backdrop_specular).unwrap_or(0.0);
            let saturate = css.and_then(|s| s.backdrop_saturate).unwrap_or(1.0);
            if blur > 0.0 || refraction > 0.0 || specular > 0.0 || saturate != 1.0 {
                dl.push(DrawCmd::BackdropFilter {
                    rect: bounds,
                    radii: CornerRadii::all(radius),
                    blur,
                    saturate,
                    refraction,
                    specular,
                });
            }
            // A focused text editor gets an accent focus ring (drawn on the box
            // edge). It's the *default* — an explicit border (element or `.lss`)
            // wins; customize focus feedback via a `&:focused { border: … }` rule.
            let focused = self.tree.flags(node).contains(NodeFlags::FOCUSED);
            let focus_border = (focused && m.on_caret_set.is_some()).then(|| Border {
                width: 2.0,
                color: crate::theme::accent(),
            });
            // `.lss` border (shorthand or longhands) wins over an element border,
            // which wins over the focus ring.
            let css_border = css.and_then(|s| match (s.border_width, s.border_color) {
                (None, None) => None,
                (w, c) => Some(Border {
                    width: w.unwrap_or(1.0) as f64,
                    color: c.unwrap_or(Color::srgb8(0, 0, 0, 0xff)),
                }),
            });
            let border = css_border.or(m.border).or(focus_border);
            // Emit the box rect for a fill *or* a border (an outline-only box has
            // a transparent fill); nodes with neither stay rect-free as before.
            if bg.is_some() || border.is_some() {
                dl.push(DrawCmd::Rect {
                    rect: bounds,
                    brush: Brush::Solid(bg.unwrap_or(Color::srgb8(0, 0, 0, 0))),
                    radii: CornerRadii::all(radius),
                    border,
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
                let (cmds, texts) = frame.into_parts();
                for cmd in cmds {
                    dl.push(cmd);
                }
                for t in texts {
                    Self::rasterize_frame_text(&mut self.text, &mut self.text_cache, dl, t);
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
                let (cmds, texts) = frame.into_parts();
                for cmd in cmds {
                    dl.push(cmd);
                }
                for t in texts {
                    Self::rasterize_frame_text(&mut self.text, &mut self.text_cache, dl, t);
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
                let mut ts = ts.clone();
                if let Some(c) = css.and_then(|s| s.color) {
                    ts.color = c;
                }
                // Text color is reused after `ts` is moved into layout (caret /
                // run brush / analysis target); capture it (Color is Copy).
                let text_color = ts.color;
                // Paint at the padded (content-box) origin so a button label
                // sits inside its padding (centred for symmetric padding) rather
                // than jammed into the border-box corner. Plain text has no
                // padding, so this is a no-op for it.
                let tx = bounds.x0 + m.pad.0;
                let ty = bounds.y0 + m.pad.1;
                // R3.4: emit a glyph run (positioned glyphs + atlas-bound coverage
                // bitmaps from the per-glyph cache) instead of a whole-string
                // sprite, so the GPU batches text through the atlas and a 1-char
                // edit re-rasterizes ≤1 glyph. `block` also drives the caret /
                // selection geometry below (same layout).
                let scale = self.scale as f32;
                // R5: reuse the cached **origin-relative** glyph run — translate to
                // (tx, ty) and intern its glyphs (cloning only ones new to this
                // frame). Skips the per-frame `glyph_run` rebuild (the dominant
                // display-list-emission cost) byte-identically — the pen rounds
                // before the origin is added, so translation commutes. Ink +
                // metrics come from the cache.
                let (run, run_rect, metrics) = {
                    let cached = self.text.shaped_run(
                        txt,
                        &ts,
                        m.wrap_width,
                        lumen_text::TextAlign::Start,
                        scale,
                    );
                    let mut run = cached.run.clone();
                    for g in &mut run.glyphs {
                        g.x += tx as f32;
                        g.y += ty as f32;
                        g.image = dl.intern_glyph_ref(&cached.images[g.image as usize]);
                    }
                    let run_rect = Rect::new(
                        cached.ink[0] as f64 + tx,
                        cached.ink[1] as f64 + ty,
                        cached.ink[2] as f64 + tx,
                        cached.ink[3] as f64 + ty,
                    );
                    (run, run_rect, cached.metrics)
                };
                // Selection highlight (behind the glyphs) for a focused editor —
                // re-shape (cached, cheap) for the selection geometry.
                if focused && m.caret_byte.is_some() {
                    if let Some((a, b)) = m.selection.filter(|(a, b)| a != b) {
                        let sel = Color::srgb8(0x1a, 0x73, 0xe8, 0x55);
                        let block =
                            self.text
                                .shaped(txt, &ts, m.wrap_width, lumen_text::TextAlign::Start);
                        for (x0, y0, x1, y1) in block.selection_rects(a, b) {
                            dl.push(DrawCmd::Rect {
                                rect: Rect::new(
                                    tx + x0 as f64,
                                    ty + y0 as f64,
                                    tx + x1 as f64,
                                    ty + y1 as f64,
                                ),
                                brush: Brush::Solid(sel),
                                radii: CornerRadii::all(0.0),
                                border: None,
                            });
                        }
                    }
                }
                // Record the glyph-ink bounds for this node so the clipping audit
                // (W0104) and ui.getLayout can compare ink vs the layout box.
                self.node_ink.insert(node, run_rect);
                self.node_text_metrics.insert(node, metrics);
                let run_id = dl.add_run(run);
                dl.push(DrawCmd::GlyphRun {
                    run: run_id,
                    brush: Brush::Solid(text_color),
                    rect: run_rect,
                });
                // Caret (in front) for a focused editor — re-shape (cached) for
                // the caret geometry.
                if let Some(caret) = m.caret_byte.filter(|_| focused) {
                    let block =
                        self.text
                            .shaped(txt, &ts, m.wrap_width, lumen_text::TextAlign::Start);
                    let (cx, cy, ch) = block.caret_pos(caret);
                    let w = 1.5;
                    dl.push(DrawCmd::Rect {
                        rect: Rect::new(
                            tx + cx as f64,
                            ty + cy as f64,
                            tx + cx as f64 + w,
                            ty + cy as f64 + ch as f64,
                        ),
                        brush: Brush::Solid(text_color),
                        radii: CornerRadii::all(0.0),
                        border: None,
                    });
                }
                // Mirror the painted text as a design-analysis target: the
                // foreground is the text's resolved color, the region its bounds.
                text_targets.push(lumen_render::TextTarget {
                    node: Some(format!("node-{}", node.index())),
                    label: Some(txt.clone()),
                    foreground: text_color,
                    region: bounds,
                });
            }
        }
        // Close any clip layers still open at the end of the pass.
        for _ in 0..clip_stack.len() {
            dl.push(DrawCmd::PopLayer);
        }
    }

    /// Rasterize a [`FrameText`] (from a canvas / custom-leaf `fill_text`) into an
    /// image blit on `dl`, anchored per its opts. A free fn over the two fields it
    /// needs (not `&mut self`) so it composes with the `&self.meta` borrow held by
    /// the paint loop. Shares the glyph cache with own-text painting.
    fn rasterize_frame_text(
        text: &mut TextEngine,
        cache: &mut HashMap<(String, u32, u32, u32, u32), RgbaImage>,
        dl: &mut DisplayList,
        t: lumen_render::canvas::FrameText,
    ) {
        use lumen_render::canvas::{AnchorX, AnchorY};
        let ts = lumen_text::TextStyle {
            font_size: t.opts.size,
            weight: t.opts.weight,
            color: t.opts.color,
            line_height: None,
            letter_spacing: 0.0,
            family: None,
        };
        let [cr, cg, cb, ca] = ts.color.to_srgb8();
        let key = (
            t.text.clone(),
            ts.font_size.to_bits(),
            ts.weight.to_bits(),
            u32::from_le_bytes([cr, cg, cb, ca]),
            0, // no wrap
        );
        let img = if let Some(cached) = cache.get(&key) {
            cached.clone()
        } else {
            let block = text.layout(&t.text, ts, &[], None, lumen_text::TextAlign::Start);
            let img = block.render(0, 0, Color::srgb8(255, 255, 255, 0));
            const CAP: usize = 512;
            if cache.len() >= CAP {
                cache.clear();
            }
            cache.insert(key, img.clone());
            img
        };
        let iw = img.width() as f64;
        let ih = img.height() as f64;
        // Offset the anchor point to the box's top-left, then snap to whole px so
        // the nearest-sampled blit stays crisp.
        let dx = match t.opts.anchor_x {
            AnchorX::Start => 0.0,
            AnchorX::Center => -iw / 2.0,
            AnchorX::End => -iw,
        };
        let dy = match t.opts.anchor_y {
            AnchorY::Top => 0.0,
            AnchorY::Middle => -ih / 2.0,
            AnchorY::Bottom => -ih,
        };
        let x = (t.pos.x + dx).round();
        let y = (t.pos.y + dy).round();
        let id = lumen_render::ImageId(dl.images.len() as u32);
        dl.images.push(img);
        dl.push(DrawCmd::Image {
            id,
            src_rect: Rect::new(0.0, 0.0, iw, ih),
            dst_rect: Rect::new(x, y, x + iw, y + ih),
            quality: lumen_render::Filter::Nearest,
        });
    }

    /// Rasterize the current build into `self.frame`, repainting only what
    /// changed since the last frame (R2). Returns the [`Damage`] applied.
    ///
    /// The retained `self.frame` always equals a full render: when nothing
    /// changed it is reused; when a sub-region changed, only that region is
    /// re-rendered (byte-identical to a full render there — R0
    /// `damage_equivalence`) and composited in, leaving the unchanged pixels
    /// (which still match) intact.
    fn paint(&mut self) -> Damage {
        let (dl, _) = self.build_display_list();
        // Layout/display list are in logical px; rasterize at physical px so the
        // frame matches a HiDPI surface 1:1 (no upscaling blur). scale 1.0 is
        // byte-identical to the unscaled path (goldens unaffected).
        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
        let bg = Color::srgb8(255, 255, 255, 255);

        // Incremental only when we have a previous display list to diff against.
        // The CPU path additionally needs the retained frame to match the target
        // size; the surface path keeps no CPU frame, so the prev-dl is enough.
        let can_incremental = self.last_dl.is_some()
            && (self.surface_attached || (self.frame.width() == pw && self.frame.height() == ph));
        let damage = if can_incremental {
            lumen_render::damage_between(self.last_dl.as_ref().unwrap(), &dl)
        } else {
            Damage::Full
        };

        if self.surface_attached {
            // Direct-to-surface (1c): no CPU rasterization. The shell presents the
            // retained `last_dl` via `present_to_surface` when `damage != None`
            // (granularity is ignored — the GPU renders the whole frame anyway).
        } else {
            match damage {
                Damage::None => { /* nothing changed — reuse self.frame */ }
                Damage::Region(r) => {
                    // Logical → physical, integer-aligned, clamped to the frame.
                    let dirty = kurbo::Rect::new(
                        (r.x0 * self.scale).floor().max(0.0),
                        (r.y0 * self.scale).floor().max(0.0),
                        (r.x1 * self.scale).ceil().min(pw as f64),
                        (r.y1 * self.scale).ceil().min(ph as f64),
                    );
                    if dirty.width() >= 1.0 && dirty.height() >= 1.0 {
                        let tile = self
                            .renderer
                            .render_damage(&dl, pw, ph, self.scale, bg, dirty);
                        self.frame
                            .overwrite_rect(dirty.x0 as u32, dirty.y0 as u32, &tile);
                    }
                }
                Damage::Full => {
                    self.frame = self.renderer.render_frame(&dl, pw, ph, self.scale, bg);
                }
            }
        }
        self.last_dl = Some(dl);
        damage
    }

    /// Replace the frame renderer with another of the *same* type `R`, then
    /// re-render. (The backend type is chosen at construction via
    /// `App::with_renderer`; this only swaps the instance — e.g. a reconfigured
    /// `Box<dyn Renderer>` when `R` is the boxed escape-hatch type.)
    pub fn set_renderer(&mut self, renderer: R) {
        self.renderer = renderer;
        self.pump();
    }

    /// The active renderer backend's name (e.g. `"cpu"`).
    pub fn renderer_name(&self) -> &'static str {
        self.renderer.name()
    }

    /// Shared reference to the background-work executor `E`. Lets a test reach a
    /// [`ManualSpawner`](lumen_core::tasks::ManualSpawner) after it has been moved
    /// into the runtime (to `run_pending` between pumps).
    pub fn executor(&self) -> &E {
        &self.executor
    }

    /// Set the host waker invoked when a background result is queued (the shell
    /// wires an event-loop wake so results schedule a frame). Headless leaves it
    /// unset; the next manual `pump` drains the deferred-op queue regardless.
    pub fn set_waker(&mut self, waker: lumen_core::tasks::WakeFn) {
        self.task_waker = Some(waker);
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
        s.deps = m.and_then(|m| (!m.deps.is_empty()).then(|| m.deps.union()));
        s.ink = self.node_ink.get(&node).copied();
        s.text_metrics =
            self.node_text_metrics
                .get(&node)
                .map(|m| lumen_core::semantics::TextMetrics {
                    line_count: m.line_count as u32,
                    box_height: m.box_height,
                    ascent: m.ascent,
                    descent: m.descent,
                    line_height: m.line_height,
                    content_height: m.content_height,
                });
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
