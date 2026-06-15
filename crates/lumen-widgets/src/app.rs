//! The application and its headless runtime (02 §8).
//!
//! `Headless::pump` runs one turn: drain input → rebuild the element tree →
//! lay out → paint to the CPU renderer → build the semantic tree. It integrates
//! lumen-core (tree/state/events/semantics), lumen-layout, lumen-render, and
//! lumen-text. Interactive state (focus/hover) is keyed by [`StableId`] so it
//! survives the from-scratch rebuild.

use crate::element::{BuildCx, Element, Handler};
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
use lumen_text::{TextEngine, TextStyle};
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
        let mut h = Headless {
            root: self.root,
            rt: Runtime::new(),
            size,
            clock_ms: 0.0,
            text: TextEngine::new(),
            tree: Tree::new(),
            meta: HashMap::new(),
            frame: RgbaImage::new(size.width as u32, size.height as u32),
            sem_root: None,
            focused_id: None,
            hovered_id: None,
            pressed: None,
            input: InputQueue::new(),
            pointer: PointerState::new(),
        };
        h.rebuild();
        h
    }
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
    on_text: Option<crate::element::TextHandler>,
    background: Option<Color>,
    corner_radius: f64,
    text: Option<(String, TextStyle)>,
    image: Option<RgbaImage>,
}

/// A headless, CPU-rendered application instance (02 §8). Drives the same input
/// queue as a real shell, so tests and the agent exercise the real paths.
pub struct Headless {
    root: Box<dyn Fn(&mut BuildCx) -> Element>,
    rt: Runtime,
    size: Size,
    clock_ms: f64,
    text: TextEngine,
    tree: Tree,
    meta: HashMap<NodeIndex, NodeMeta>,
    frame: RgbaImage,
    sem_root: Option<SemanticsNode>,
    focused_id: Option<StableId>,
    hovered_id: Option<StableId>,
    pressed: Option<NodeIndex>,
    input: InputQueue,
    pointer: PointerState,
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

    /// The most recent rendered frame.
    pub fn screenshot(&mut self) -> RgbaImage {
        self.frame.clone()
    }

    /// The current virtual-clock time (ms).
    pub fn now_ms(&self) -> f64 {
        self.clock_ms
    }

    /// Advance the virtual clock by `ms`.
    pub fn advance_clock(&mut self, ms: f64) {
        self.clock_ms += ms;
    }

    /// The semantics document as JSON (`lumen-semantics/1`, 03 §1).
    pub fn semantics_json(&self) -> serde_json::Value {
        self.semantics_doc().to_json(false)
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
                let target = self.tree.hit_test(pe.pos);
                self.hovered_id = target.and_then(|t| self.meta.get(&t).and_then(|m| m.id.clone()));
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

    fn rebuild(&mut self) {
        let root_el = {
            let mut cx = BuildCx::new(&self.rt, self.clock_ms);
            (self.root)(&mut cx)
        };

        let mut tree = Tree::new();
        let mut layout = LayoutTree::new();
        let mut meta = HashMap::new();
        let mut built: Vec<(NodeIndex, LayoutNode)> = Vec::new();
        let (_root_node, root_lnode) = self.build_node(
            &root_el,
            &mut tree,
            &mut layout,
            &mut meta,
            &mut built,
            None,
        );

        layout.compute(root_lnode, self.size);
        for (node, lnode) in &built {
            tree.set_bounds(*node, layout.bounds(*lnode));
        }

        self.tree = tree;
        self.meta = meta;
        self.frame = self.paint();
        self.sem_root = Some(self.build_semantics(self.tree.root()));
    }

    #[allow(clippy::too_many_arguments)]
    fn build_node(
        &mut self,
        el: &Element,
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
            || el.text.is_some()
            || el.image.is_some()
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
        let mut style = el.style.clone();
        if let Some((txt, ts)) = &el.text {
            let block = self
                .text
                .layout(txt, *ts, &[], None, lumen_text::TextAlign::Start);
            style.width = Dim::px(block.width().ceil());
            style.height = Dim::px(block.height().ceil());
        }

        let child_built: Vec<(NodeIndex, LayoutNode)> = el
            .children
            .iter()
            .map(|c| self.build_node(c, tree, layout, meta, built, Some(node)))
            .collect();
        let child_lnodes: Vec<LayoutNode> = child_built.iter().map(|(_, l)| *l).collect();
        let lnode = if child_lnodes.is_empty() {
            layout.leaf(style)
        } else {
            layout.container(style, &child_lnodes)
        };

        meta.insert(
            node,
            NodeMeta {
                id: el.id.clone(),
                role: el.role,
                label: el.label.clone(),
                value: el.value.clone(),
                classes: el.classes.clone(),
                actions: el.actions.clone(),
                states: el.states.clone(),
                scroll: el.scroll,
                focusable: el.focusable,
                elide: el.elide_semantics,
                on_click: el.on_click.clone(),
                on_wheel: el.on_wheel.clone(),
                on_drag: el.on_drag.clone(),
                on_text: el.on_text.clone(),
                background: el.background,
                corner_radius: el.corner_radius,
                text: el.text.clone(),
                image: el.image.clone(),
            },
        );
        built.push((node, lnode));
        (node, lnode)
    }

    // --- paint --------------------------------------------------------------

    fn paint(&mut self) -> RgbaImage {
        let mut dl = DisplayList::new();
        let order = self.tree.document_order();
        for node in order {
            let bounds = self.tree.bounds(node);
            let Some(m) = self.meta.get(&node) else {
                continue;
            };
            if let Some(bg) = m.background {
                dl.push(DrawCmd::Rect {
                    rect: bounds,
                    brush: Brush::Solid(bg),
                    radii: CornerRadii::all(m.corner_radius),
                    border: None,
                });
            }
            if let Some(img) = &m.image {
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
            if let Some((txt, ts)) = &m.text {
                let block = self
                    .text
                    .layout(txt, *ts, &[], None, lumen_text::TextAlign::Start);
                let img = block.render(0, 0, Color::srgb8(255, 255, 255, 0)); // transparent bg
                let iw = img.width() as f64;
                let ih = img.height() as f64;
                let id = lumen_render::ImageId(dl.images.len() as u32);
                dl.images.push(img);
                dl.push(DrawCmd::Image {
                    id,
                    src_rect: Rect::new(0.0, 0.0, iw, ih),
                    dst_rect: Rect::new(bounds.x0, bounds.y0, bounds.x0 + iw, bounds.y0 + ih),
                    quality: lumen_render::Filter::Nearest,
                });
            }
        }
        cpu::render(
            &dl,
            self.size.width as u32,
            self.size.height as u32,
            Color::srgb8(255, 255, 255, 255),
        )
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

/// Helper: the center point of a rect (for synthesized clicks).
pub fn center(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

/// Re-export so callers can build the default window background.
pub const WINDOW_BG: Color = Color::WHITE;

/// A default style alias used by examples.
pub type Style = LayoutStyle;
