//! **Prototype (WT-EXP).** Lowering a widget *straight into the tree*, with no
//! `Element` in between.
//!
//! # What this is testing
//!
//! Today a widget produces an [`Element`](crate::Element) — 1072 bytes — and
//! `build_node` then reads 41 of its fields back out and copies them into the
//! two structures that actually keep the data: the SoA `Tree` (geometry, flags,
//! links) and a per-node `NodeMeta` side table (semantics, handlers, paint
//! props). The `Element` is dropped immediately afterwards.
//!
//! Measured on a 500-row app, the whole `Element` tree is **3.07 MB alive at
//! once**, 16.8% of the app's RSS, purely as a staging buffer.
//!
//! [`Direct`] removes the staging buffer: the widget receives the sink and
//! writes its own fields into it. Nothing uniform is materialized, so a widget
//! costs what its own data costs and no more.
//!
//! # Why the comparison is fair
//!
//! Both paths end at the *same destination writes* — insert a node, compute
//! `NodeFlags`, create a taffy node, insert a meta record. The only thing that
//! varies is whether an `Element` is materialized and read back in between.
//! [`lower_element`] and the [`Direct`] impls are held to that: `lowered_eq`
//! asserts the two produce equivalent trees before either is timed.
//!
//! # What it deliberately leaves out
//!
//! The `cx.scope` memo machinery, the `.lss` cascade, overlay/z handling and
//! damage tracking. All of them sit on the far side of the marshalling step and
//! are identical between the two paths, so including them would add noise to
//! both arms without changing the delta.

use crate::element::NodeContent;
use crate::Element;
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::tree::{NodeFlags, Tree};
use lumen_core::{Color, NodeIndex, StableId};
use lumen_layout::{LayoutNode, LayoutStyle, LayoutTree};
use lumen_render::Border;
use lumen_text::TextStyle;
use std::collections::HashMap;

/// The per-node side table, mirroring the observable subset of `lumen-app`'s
/// private `NodeMeta`. This is what the agent, `ui.lint`, `lumen-test` and the
/// accessibility bridge read — never the `Element`.
pub struct Meta {
    /// Stable id.
    pub id: Option<StableId>,
    /// Accessible role.
    pub role: Role,
    /// Accessible name.
    pub label: String,
    /// Current value (inputs, sliders, progress).
    pub value: Option<String>,
    /// `.lss` classes.
    pub classes: Vec<String>,
    /// Interned classes — the allocation-free form. Four bytes each, inline up
    /// to three, and a `&'static str` class costs nothing after its first use.
    pub class_syms: ClassSet,
    /// Structured identity — eight bytes, no string minted.
    pub node_id: Option<NodeId>,
    /// Advertised actions.
    pub actions: Vec<Action>,
    /// Semantic states.
    pub states: Vec<SemState>,
    /// Keyboard-focusable.
    pub focusable: bool,
    /// Elided from semantics (pure layout).
    pub elide: bool,
    /// Disabled — inert, and matched by `:disabled`.
    pub disabled: bool,
    /// Mid-transition, so a span containing it must not be spliced.
    pub animating: bool,
    /// Click handler.
    pub on_click: Option<crate::Handler>,
    /// Background fill.
    pub background: Option<Color>,
    /// Border.
    pub border: Option<Border>,
    /// Corner radius.
    pub corner_radius: f64,
    /// Leaf content.
    pub content: NodeContent,
    /// The style handed to taffy: the widget's own, with the cascade folded on.
    pub layout_style: LayoutStyle,
}

impl Default for Meta {
    fn default() -> Meta {
        Meta {
            id: None,
            role: Role::Generic,
            label: String::new(),
            value: None,
            classes: Vec::new(),
            class_syms: ClassSet::default(),
            node_id: None,
            actions: Vec::new(),
            states: Vec::new(),
            focusable: false,
            elide: false,
            disabled: false,
            animating: false,
            on_click: None,
            background: None,
            border: None,
            corner_radius: 0.0,
            content: NodeContent::None,
            layout_style: LayoutStyle::default(),
        }
    }
}

/// The destination both paths write into: the SoA tree, the layout tree, and
/// the per-node side table.
pub struct TreeSink {
    /// Node arena + geometry + flags + links.
    pub tree: Tree,
    /// The taffy layout tree.
    pub layout: LayoutTree,
    /// Per-node semantics/handlers/paint, stored as columns.
    ///
    /// The in-flight record on the `open` stack is still an AoS `Meta` — it is
    /// a builder buffer, not storage, and one node is in flight at a time. It
    /// is committed into the columns when the node closes.
    pub meta: MetaStore,
    /// The stylesheet environment, if any.
    pub(crate) styles: Option<StyleEnv>,
    /// Engine-side focus/hover, which the cascade matches on.
    pub(crate) visual: VisualState,
    /// B.1: the ancestor chain, for descendant and `>` selectors.
    pub(crate) desc_stack: Vec<NodeDesc>,
    /// Resolved layout properties, held between `resolve` and `end`.
    pub(crate) pending_css: HashMap<NodeIndex, Style>,
    /// W1: depth of the enclosing disabled subtree, for inherited `:disabled`.
    pub(crate) disabled_depth: usize,
    /// Spans retained from the previous frame, by scope key.
    pub(crate) prev_spans: HashMap<u64, SpanRec>,
    /// Spans recorded this frame.
    pub(crate) spans: HashMap<u64, SpanRec>,
    /// The previous frame's root, for the sweep.
    pub(crate) old_root: Option<NodeIndex>,
    /// What this frame did.
    pub(crate) stats: FrameStats,
    /// The interning table for classes and id names.
    pub symbols: Symbols,
    /// B.2b: the `.container()` nodes built this frame, in build order.
    pub(crate) container_nodes: Vec<NodeIndex>,
    /// Their sizes from the previous layout, by the same order.
    pub(crate) container_prev: Vec<(f64, f64)>,
    /// Build-time stack of the nearest enclosing container's size.
    pub(crate) container_stack: Vec<Option<(f64, f64)>>,
    /// Bumped by a tier-2 code swap; invalidates every retained span.
    pub(crate) build_gen: u64,
    /// Parsed `@keyframes` timelines by name.
    pub(crate) keyframes: HashMap<String, Timeline>,
    /// Running timelines: id -> (start_ms, finished).
    pub(crate) key_anims: HashMap<StableId, (f64, bool)>,
    /// Suppress animation (accessibility).
    pub(crate) reduced_motion: bool,
    /// Bumped whenever a timeline starts or finishes.
    ///
    /// The span scan that decides whether a memo hit is refused is O(span), and
    /// it runs for *every* span the moment any animation is live — so one
    /// spinner made five hundred scopes each walk their subtree, every frame.
    /// Measured, that took a fully-animated frame to **1.8x the cost of not
    /// memoizing at all**: the scan plus the rebuild. The verdict only changes
    /// when the registry does, so it is cached against this counter and the
    /// steady state costs one comparison.
    pub(crate) anim_epoch: u64,
    /// Animation clock, ms.
    pub(crate) clock_ms: f64,
    /// Running transitions by node id.
    pub(crate) anims: HashMap<StableId, Anim>,
    /// Depth of the enclosing overlay subtree.
    pub(crate) overlay_depth: usize,
    /// The text engine, when text leaves should be measured.
    pub(crate) text: Option<Box<lumen_text::TextEngine>>,
    /// Nodes begun but not yet ended, innermost last.
    ///
    /// The first cut kept every record in `meta` from `begin` and reached it
    /// through `meta.get_mut(&n)` on *every* property setter — eight hashed
    /// lookups for a `Button`, where the `Element` path writes struct fields
    /// and inserts once. That alone made direct lowering measurably slower
    /// than the path it was supposed to beat.
    pub(crate) open: Vec<(NodeIndex, Meta, bool)>,
}

impl Default for TreeSink {
    fn default() -> TreeSink {
        TreeSink::new()
    }
}

impl TreeSink {
    /// An empty sink.
    pub fn new() -> TreeSink {
        TreeSink {
            tree: Tree::new(),
            layout: LayoutTree::new(),
            meta: MetaStore::default(),
            styles: None,
            visual: VisualState::default(),
            desc_stack: Vec::new(),
            pending_css: HashMap::new(),
            disabled_depth: 0,
            prev_spans: HashMap::new(),
            spans: HashMap::new(),
            old_root: None,
            stats: FrameStats::default(),
            overlay_depth: 0,
            symbols: Symbols::default(),
            container_nodes: Vec::new(),
            container_prev: Vec::new(),
            container_stack: Vec::new(),
            build_gen: 0,
            keyframes: HashMap::new(),
            key_anims: HashMap::new(),
            reduced_motion: false,
            anim_epoch: 0,
            clock_ms: 0.0,
            anims: HashMap::new(),
            text: None,
            open: Vec::new(),
        }
    }

    /// Allocate a node under `parent` (or as the root) and start its record.
    pub fn begin(&mut self, parent: Option<NodeIndex>, role: Role) -> NodeIndex {
        let n = match parent {
            None => {
                let n = self.tree.insert_orphan();
                self.tree.set_root(n);
                n
            }
            Some(p) => self.tree.insert_child(p),
        };
        if self.overlay_depth > 0 {
            self.tree.set_z(n, OVERLAY_Z);
        }
        self.open.push((
            n,
            Meta {
                role,
                ..Meta::default()
            },
            false,
        ));
        n
    }

    /// The record under construction. Innermost-last, so the common case is
    /// the last slot and no hashing happens at all.
    fn at(&mut self, n: NodeIndex) -> &mut Meta {
        let i = self
            .open
            .iter()
            .rposition(|(k, _, _)| *k == n)
            .expect("node begun but not ended");
        &mut self.open[i].1
    }

    /// Mark a node as having resolved, so `end` knows to pop the ancestor it
    /// pushed. Inferring this from a stored style was wrong: with no
    /// stylesheet loaded `resolve` still pushes an ancestor but stores nothing,
    /// so the pop never happened and the chain grew without bound — caught by
    /// `assert_balanced`, which is what it is for.
    fn mark_resolved(&mut self, n: NodeIndex) {
        let i = self
            .open
            .iter()
            .rposition(|(k, _, _)| *k == n)
            .expect("node begun but not ended");
        self.open[i].2 = true;
    }

    /// Whether `n` resolved.
    fn did_resolve(&self, n: NodeIndex) -> bool {
        self.open
            .iter()
            .rposition(|(k, _, _)| *k == n)
            .map(|i| self.open[i].2)
            .unwrap_or(false)
    }

    /// Read the record under construction.
    fn peek(&self, n: NodeIndex) -> &Meta {
        let i = self
            .open
            .iter()
            .rposition(|(k, _, _)| *k == n)
            .expect("node begun but not ended");
        &self.open[i].1
    }

    /// Accessible name.
    pub fn label(&mut self, n: NodeIndex, s: String) {
        self.at(n).label = s;
    }
    /// Current value.
    pub fn value(&mut self, n: NodeIndex, s: String) {
        self.at(n).value = Some(s);
    }
    /// Stable id.
    pub fn id(&mut self, n: NodeIndex, id: StableId) {
        self.at(n).id = Some(id);
    }
    /// Append a class (string form).
    pub fn class(&mut self, n: NodeIndex, c: String) {
        self.at(n).classes.push(c);
    }
    /// Append an interned class — no allocation for a `&'static str`.
    pub fn class_sym(&mut self, n: NodeIndex, c: Sym) {
        self.at(n).class_syms.push(c);
    }
    /// Set the structured identity — no string minted.
    pub fn node_id(&mut self, n: NodeIndex, id: NodeId) {
        self.at(n).node_id = Some(id);
    }
    /// Intern a `&'static str`.
    pub fn sym(&mut self, s: &'static str) -> Sym {
        self.symbols.intern_static(s)
    }
    /// Advertise actions.
    pub fn actions(&mut self, n: NodeIndex, a: Vec<Action>) {
        self.at(n).actions = a;
    }
    /// Semantic states.
    pub fn states(&mut self, n: NodeIndex, s: Vec<SemState>) {
        self.at(n).states = s;
    }
    /// Keyboard focusable.
    pub fn focusable(&mut self, n: NodeIndex, yes: bool) {
        self.at(n).focusable = yes;
    }
    /// Elide from semantics.
    pub fn elide(&mut self, n: NodeIndex, yes: bool) {
        self.at(n).elide = yes;
    }
    /// Mark the node disabled (inert, and matched by `:disabled`).
    pub fn disabled(&mut self, n: NodeIndex, yes: bool) {
        self.at(n).disabled = yes;
    }
    /// Enter a disabled subtree, so descendants match `:disabled` too (W1).
    pub fn enter_disabled(&mut self) {
        self.disabled_depth += 1;
    }
    /// Leave a disabled subtree.
    pub fn exit_disabled(&mut self) {
        self.disabled_depth = self.disabled_depth.saturating_sub(1);
    }
    /// Click handler.
    pub fn on_click(&mut self, n: NodeIndex, h: crate::Handler) {
        self.at(n).on_click = Some(h);
    }
    /// Background fill.
    pub fn background(&mut self, n: NodeIndex, c: Color) {
        self.at(n).background = Some(c);
    }
    /// Border.
    pub fn border(&mut self, n: NodeIndex, b: Border) {
        self.at(n).border = Some(b);
    }
    /// Corner radius.
    pub fn corner_radius(&mut self, n: NodeIndex, r: f64) {
        self.at(n).corner_radius = r;
    }
    /// A text leaf.
    pub fn text(&mut self, n: NodeIndex, s: String, ts: TextStyle) {
        self.at(n).content = NodeContent::Text(s, ts);
    }

    /// Close the node: compute its flags and create its layout node.
    ///
    /// Mirrors `build_node`'s flag derivation exactly — hit-testable if it
    /// paints or handles input, plus the focusable/disabled bits.
    pub fn end(
        &mut self,
        n: NodeIndex,
        style: &LayoutStyle,
        children: &[LayoutNode],
        disabled: bool,
    ) -> LayoutNode {
        let m = self.peek(n);
        let interactive = m.background.is_some()
            || m.on_click.is_some()
            || !matches!(m.content, NodeContent::None)
            || m.focusable;
        let mut flags = NodeFlags::VISIBLE;
        if interactive {
            flags |= NodeFlags::HIT_TESTABLE;
        }
        if m.focusable {
            flags |= NodeFlags::FOCUSABLE;
        }
        if disabled {
            flags |= NodeFlags::DISABLED;
        }
        self.tree.set_flags(n, flags);
        // Fold the cascade's layout properties onto the widget's own style —
        // composition, where `build_node` mutated the element in place.
        let mut styled;
        if self.did_resolve(n) {
            self.desc_stack.pop();
        }
        let mut style = match self.pending_css.get(&n) {
            None => style.clone(),
            Some(css) => {
                styled = style.clone();
                apply_css_layout(&mut styled, css);
                styled
            }
        };
        // P1: a text leaf sizes its own box here, where the widget's style, the
        // cascade's overrides and the content are all finally known.
        if self.text.is_some() {
            self.measure_text(n, &mut style);
        }
        self.pending_css.remove(&n);
        let style = &style;
        self.at(n).layout_style = style.clone();
        let lnode = if children.is_empty() {
            self.layout.leaf_ref(style)
        } else {
            self.layout.container_ref(style, children)
        };
        self.tree.set_lnode(n, lnode.raw());
        // The record moves into the columns exactly once, when the node closes.
        let i = self
            .open
            .iter()
            .rposition(|(k, _, _)| *k == n)
            .expect("node begun but not ended");
        let (_, meta, _) = self.open.remove(i);
        self.meta.commit(n, meta);
        lnode
    }
}

/// A widget that lowers **straight into the tree**, with no `Element`.
///
/// The counterpart of [`Widget::build`](crate::Widget::build): same data, same
/// destination, without the uniform 1072-byte staging record in between.
pub trait Direct {
    /// Write this widget (and its subtree) into `out` under `parent`.
    ///
    /// Takes `self: Box<Self>`, not `self`. That is the difference between a
    /// trait that can describe a *leaf* and one that can describe a **tree**:
    /// `fn lower(self, ..)` is not callable through `Box<dyn Direct>` (E0161 —
    /// `dyn Direct` has no statically known size to move), so a container could
    /// only ever hold children whose types it knew at compile time. Every real
    /// view is `column(vec![heterogeneous…])`, so that limit is the whole
    /// problem, not an edge case.
    ///
    /// The box costs one small allocation per node — a `Label` is 72 bytes,
    /// against the 784-byte `Element` it replaces — and it buys dynamic
    /// children, which is what makes `Element` removable rather than merely
    /// smaller.
    fn lower(
        self: Box<Self>,
        out: &mut TreeSink,
        parent: Option<NodeIndex>,
    ) -> (NodeIndex, LayoutNode);
}

/// A boxed child, the unit a container holds.
pub type Node = Box<dyn Direct>;

/// Box a widget as a [`Node`], so `vec![node(a), node(b)]` composes widgets of
/// different types.
pub fn node<W: Direct + 'static>(w: W) -> Node {
    Box::new(w)
}

/// Walk an already-built `Element` into the same sink — the path that exists
/// today, reduced to the writes `build_node` performs.
///
/// Kept deliberately close to `build_node`'s structure so the comparison is
/// against what the engine really does, not a caricature of it.
pub fn lower_element(
    el: Element,
    out: &mut TreeSink,
    parent: Option<NodeIndex>,
) -> (NodeIndex, LayoutNode) {
    let n = out.begin(parent, el.role);
    let Element {
        id,
        label,
        value,
        classes,
        actions,
        states,
        focusable,
        elide_semantics,
        on_click,
        background,
        border,
        corner_radius,
        content,
        style,
        disabled,
        children,
        ..
    } = el;

    {
        let m = out.at(n);
        m.id = id;
        m.label = label;
        m.value = value;
        m.classes = classes;
        m.actions = actions;
        m.states = states;
        m.focusable = focusable;
        m.elide = elide_semantics;
        m.on_click = on_click;
        m.background = background;
        m.border = border;
        m.corner_radius = corner_radius;
        m.content = content;
        m.disabled = disabled;
    }

    // The Element path resolves at the same point `build_node` does: after the
    // node's own props are known, before its children become descendants.
    out.resolve(n);

    let mut child_lnodes = Vec::with_capacity(children.len());
    for c in children {
        let (_, ln) = lower_element(c, out, Some(n));
        child_lnodes.push(ln);
    }
    let lnode = out.end(n, &style, &child_lnodes, disabled);
    (n, lnode)
}

/// Compare two lowerings for equivalence — the guard that keeps the benchmark
/// honest. If the direct path skipped work the Element path does, this fails.
pub fn lowered_eq(a: &TreeSink, b: &TreeSink) -> Result<(), String> {
    if a.tree.len() != b.tree.len() {
        return Err(format!("node count {} vs {}", a.tree.len(), b.tree.len()));
    }
    // Walk the tree's live nodes rather than a map's arbitrary order, so a
    // mismatch names the node it happened at.
    for n in a.tree.iter_live() {
        if !a.meta.contains(n) {
            continue;
        }
        if !b.meta.contains(n) {
            return Err(format!("{n:?} missing from the other lowering"));
        }
        if a.meta.role(n) != b.meta.role(n) {
            return Err(format!(
                "{n:?} role {:?} vs {:?}",
                a.meta.role(n),
                b.meta.role(n)
            ));
        }
        if a.meta.label(n) != b.meta.label(n) {
            return Err(format!(
                "{n:?} label {:?} vs {:?}",
                a.meta.label(n),
                b.meta.label(n)
            ));
        }
        if a.meta.value(n) != b.meta.value(n) {
            return Err(format!("{n:?} value differs"));
        }
        if a.meta.classes(n) != b.meta.classes(n) {
            return Err(format!(
                "{n:?} classes {:?} vs {:?}",
                a.meta.classes(n),
                b.meta.classes(n)
            ));
        }
        if a.meta.actions(n) != b.meta.actions(n) {
            return Err(format!("{n:?} actions differ"));
        }
        if a.meta.states(n) != b.meta.states(n) {
            return Err(format!("{n:?} states differ"));
        }
        if a.meta.flags(n) != b.meta.flags(n) {
            return Err(format!(
                "{n:?} flags {:?} vs {:?}",
                a.meta.flags(n),
                b.meta.flags(n)
            ));
        }
        if a.meta.on_click(n).is_some() != b.meta.on_click(n).is_some() {
            return Err(format!("{n:?} on_click presence differs"));
        }
        if a.tree.flags(n) != b.tree.flags(n) {
            return Err(format!(
                "{n:?} tree flags {:?} vs {:?}",
                a.tree.flags(n),
                b.tree.flags(n)
            ));
        }
    }
    Ok(())
}

// --- the widgets, lowering themselves -------------------------------------
//
// These live here rather than in each widget's file so the prototype stays in
// one place; in a real conversion each would sit beside its `Widget` impl and
// replace it. They are written to produce exactly what `Widget::build` +
// `lower_element` produce, which `lowered_eq` enforces.

use crate::widget::Common;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection};

/// Fold a `Common` into a node that has already been begun.
fn apply_common(out: &mut TreeSink, n: NodeIndex, common: Common) -> bool {
    let (id, classes, background, style_override, disabled) = common.into_parts();
    if let Some(id) = id {
        out.id(n, id);
    }
    for c in classes {
        out.class(n, c);
    }
    if let Some(bg) = background {
        out.background(n, bg);
    }
    // The prototype does not model `.style()`/`.css()` overrides beyond the
    // layout one; neither is used by the benchmarked widgets.
    let _ = style_override;
    disabled
}

impl Direct for crate::Label {
    fn lower(
        self: Box<Self>,
        out: &mut TreeSink,
        parent: Option<NodeIndex>,
    ) -> (NodeIndex, LayoutNode) {
        let this = *self;
        let (text, style, width, common) = this.into_parts();
        let (s, _dyn_text) = text.into_parts();
        let (d, disabled) = out.node(parent, Role::Text).common(common);
        let node = d.label(s.clone()).text(s, style).resolve();
        let n = node.index();
        let mut ls = LayoutStyle::default();
        if let Some(px) = width {
            ls.width = Dim::px(px);
        }
        let ln = node.end(&ls, &[], disabled);
        (n, ln)
    }
}

impl Direct for crate::Button {
    fn lower(
        self: Box<Self>,
        out: &mut TreeSink,
        parent: Option<NodeIndex>,
    ) -> (NodeIndex, LayoutNode) {
        let this = *self;
        let (label, on_press, fill, ink, common) = this.into_parts();
        let (s, _dyn_text) = label.into_parts();
        let (d, disabled) = out.node(parent, Role::Button).common(common);
        let mut d = d
            .label(s.clone())
            .actions(vec![Action::Click, Action::Focus])
            .focusable(true)
            .background(fill)
            .corner_radius(8.0)
            .text(
                s,
                TextStyle {
                    font_size: 15.0,
                    weight: 600.0,
                    color: ink,
                    ..TextStyle::default()
                },
            );
        if let Some(h) = on_press {
            d = d.on_click(h);
        }
        let node = d.resolve();
        let n = node.index();
        let ls = LayoutStyle {
            padding: Edges {
                left: Dim::px(16.0),
                right: Dim::px(16.0),
                top: Dim::px(9.0),
                bottom: Dim::px(9.0),
            },
            ..LayoutStyle::default()
        };
        let ln = node.end(&ls, &[], disabled);
        (n, ln)
    }
}

impl Direct for crate::ProgressBar {
    fn lower(
        self: Box<Self>,
        out: &mut TreeSink,
        parent: Option<NodeIndex>,
    ) -> (NodeIndex, LayoutNode) {
        let this = *self;
        let (frac, width, height, ink, common) = this.into_parts();
        // The ordering that used to be a comment is now the only thing that
        // compiles: `common` lands on `Declaring`, and the fill child is only
        // reachable from the `Open` this `resolve` returns.
        let (d, disabled) = out.node(parent, Role::Progress).common(common);
        let mut node = d
            .value(format!("{:.0}%", frac * 100.0))
            .background(Color::srgb8(0xe3, 0xe6, 0xeb, 0xff))
            .corner_radius(5.0)
            .resolve();
        let n = node.index();

        let fill = node
            .begin_child(Role::Generic)
            .elide(true)
            .class("fill")
            .background(ink)
            .corner_radius(5.0)
            .resolve();
        let fill_ln = fill.end(
            &LayoutStyle {
                width: Dim::pct(frac as f32),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            &[],
            false,
        );

        let ls = LayoutStyle {
            width: Dim::px(width),
            height: Dim::px(height),
            ..LayoutStyle::default()
        };
        let ln = node.end(&ls, &[fill_ln], disabled);
        (n, ln)
    }
}

/// A column of arbitrary children, lowered with no `Element` anywhere.
///
/// This is the piece the prototype was missing, and the reason it could only
/// ever demonstrate leaves. `begin_row` below carries the assumption that
/// retired it — "the child widgets are known statically at each site" — which
/// is true of a hand-written composite and false of every real view, all of
/// which are `column(vec![…])` over a heterogeneous list.
///
/// Holding `Vec<Node>` is what makes the trait describe a *tree*: the children
/// are boxed widgets, each still only as big as its own data (a `Label` is 72
/// bytes) rather than a uniform 784-byte `Element`, and none of them is
/// materialized before it is written.
pub struct Column {
    kids: Vec<Node>,
    gap: f32,
    padding: f32,
}

impl Column {
    /// A column over `kids`.
    pub fn new(kids: Vec<Node>) -> Column {
        Column {
            kids,
            gap: 0.0,
            padding: 0.0,
        }
    }

    /// Space between children, in logical px.
    pub fn gap(mut self, gap: f32) -> Column {
        self.gap = gap;
        self
    }

    /// Padding inside the column, in logical px.
    pub fn padding(mut self, padding: f32) -> Column {
        self.padding = padding;
        self
    }
}

impl Direct for Column {
    fn lower(
        self: Box<Self>,
        out: &mut TreeSink,
        parent: Option<NodeIndex>,
    ) -> (NodeIndex, LayoutNode) {
        let this = *self;
        let node = out.node(parent, Role::Group).elide(true).resolve();
        let n = node.index();
        let mut node = node;
        // The children are lowered *while this node is open*, so it is on the
        // ancestor stack for their cascade — the ordering the typestate guards
        // exist to enforce.
        let child_lns = node.children(this.kids);
        let ls = LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Dim::px(this.gap),
            padding: Edges::all(Dim::px(this.padding)),
            ..LayoutStyle::default()
        };
        let ln = node.end(&ls, &child_lns, false);
        (n, ln)
    }
}

/// Begin a row box. Children are lowered directly into it by the caller, then
/// [`TreeSink::end`] closes it — no boxed closures.
///
/// Superseded for real views by [`Column`], which takes a `Vec<Node>`; this
/// stays as the hand-written static-arity form.
pub fn begin_row(out: &mut TreeSink, parent: Option<NodeIndex>) -> NodeIndex {
    let n = out.begin(parent, Role::Group);
    out.elide(n, true);
    n
}

/// The layout style [`begin_row`]'s box closes with.
pub fn row_style(gap: f32, padding: f32) -> LayoutStyle {
    LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        padding: Edges::all(Dim::px(padding)),
        row_gap: Dim::px(gap),
        column_gap: Dim::px(gap),
        align_items: Some(Align::Center),
        ..LayoutStyle::default()
    }
}

// --- the .lss cascade, composed instead of mutated -------------------------
//
// This is the part the first prototype dodged. Today the cascade runs inside
// `build_node` and *writes into* the element — `apply_css_to_element(&mut el,
// &css)` — because `el` is sitting right there between the widget and taffy.
// With no element there is nothing to write into, so the question is whether
// the cascade can compose instead: resolve, then fold the result onto the
// style handed to taffy and the paint props handed to the side table.
//
// It can, and `apply_css_to_element` was already the shape of the answer — a
// pure function from `Style` onto a target. Splitting the target into
// `(LayoutStyle, Meta)` is mechanical; nothing about it needed an `Element`.
//
// The one real constraint deferral imposes: the cascade's *inputs* (id,
// classes, role, semantic states, disabled) must be declared before the node's
// children are lowered, because this node's `NodeDesc` becomes their ancestor
// for descendant and `>` selectors. That is a natural fit for a builder —
// `resolve()` sits between the widget declaring itself and its children being
// written — but it is a rule a widget author can now break, where before the
// element made the ordering impossible to get wrong.

use lumen_style::{MediaContext, NodeDesc, Style, StyleSource, Tokens};

/// The stylesheet environment a [`TreeSink`] resolves against.
pub struct StyleEnv {
    /// A hash of the sheet's *source*, identifying this revision.
    ///
    /// Content-addressed on purpose: keying the splice guard on "someone called
    /// `set_stylesheet`" would make a no-op save cost a full rebuild, and a file
    /// watcher that fires twice cost two.
    pub gen: u128,
    /// Parsed sheets, in cascade order.
    pub sources: Vec<StyleSource>,
    /// `--token` values.
    pub tokens: Tokens,
    /// Window/container context for `@media`.
    pub media: MediaContext,
}

impl StyleEnv {
    /// Parse `src` into an environment, or return its diagnostics.
    ///
    /// Mirrors `set_stylesheet`'s contract: a rejected edit yields nothing and
    /// the caller keeps the previous sheet live, so a typo mid-edit cannot
    /// blank the screen.
    pub fn from_source(src: &str) -> Result<StyleEnv, Vec<lumen_core::Diagnostic>> {
        use std::hash::Hash;
        let (sheet, diags) = lumen_style::parse("app.lss", src);
        if lumen_style::has_errors(&diags) {
            return Err(diags);
        }
        let mut h = lumen_core::identity::IdHasher::new();
        src.hash(&mut h);
        Ok(StyleEnv {
            gen: h.finish128(),
            sources: vec![StyleSource {
                sheet,
                origin: lumen_style::Origin::App,
            }],
            tokens: Tokens::default(),
            media: MediaContext::default(),
        })
    }
}

/// Engine-side visual state the cascade needs but the widget does not own.
#[derive(Default)]
pub struct VisualState {
    /// The focused node's id, if any.
    pub focused: Option<StableId>,
    /// The hovered node's id, if any.
    pub hovered: Option<StableId>,
}

/// Fold a resolved [`Style`]'s layout properties onto a [`LayoutStyle`].
///
/// The layout half of `apply_css_to_element`, with the element taken out of it.
pub fn apply_css_layout(ls: &mut LayoutStyle, css: &Style) {
    if let Some(d) = css.display {
        ls.display = d;
    }
    if let Some(f) = css.flex_direction {
        ls.flex_direction = f;
    }
    if let Some(w) = css.width {
        ls.width = w;
    }
    if let Some(h) = css.height {
        ls.height = h;
    }
    if let Some(g) = css.gap {
        ls.row_gap = g;
        ls.column_gap = g;
    }
    if let Some(g) = css.row_gap {
        ls.row_gap = g;
    }
    if let Some(g) = css.column_gap {
        ls.column_gap = g;
    }
    if let Some(a) = css.justify_content {
        ls.justify_content = Some(a);
    }
    if let Some(a) = css.align_items {
        ls.align_items = Some(a);
    }
    if let Some(a) = css.align_self {
        ls.align_self = Some(a);
    }
    if let Some(w) = css.flex_wrap {
        ls.flex_wrap = w;
    }
    if let Some(n) = css.flex_grow {
        ls.flex_grow = n;
    }
    if let Some(n) = css.flex_shrink {
        ls.flex_shrink = n;
    }
}

/// Fold a resolved [`Style`]'s paint properties onto a node's record.
///
/// The paint half. Text properties reach the node's own `TextStyle`, which is
/// why the widget must have declared its content before `resolve` runs —
/// measurement happens after this, and it has to measure the styled text.
pub fn apply_css_paint(m: &mut Meta, css: &Style) {
    if let Some(c) = css.background {
        m.background = Some(c);
    }
    if let Some(r) = css.border_radius {
        m.corner_radius = r as f64;
    }
    if let NodeContent::Text(_, ts) = &mut m.content {
        if let Some(c) = css.color {
            ts.color = c;
        }
        if let Some(px) = css.font_size {
            ts.font_size = px;
        }
        if let Some(w) = css.font_weight {
            ts.weight = w as f32;
        }
    }
}

impl TreeSink {
    /// Attach a stylesheet environment; nodes resolved after this participate
    /// in the cascade.
    pub fn with_styles(mut self, env: StyleEnv, visual: VisualState) -> TreeSink {
        self.styles = Some(env);
        self.visual = visual;
        self
    }

    /// Resolve `n`'s `.lss` rules and push it as an ancestor for its children.
    ///
    /// Called after the widget has declared itself and **before** it lowers its
    /// children. The resolved paint lands immediately; the resolved layout is
    /// held until [`end`](TreeSink::end) folds it onto the widget's own style.
    pub fn resolve(&mut self, n: NodeIndex) {
        self.mark_resolved(n);
        let Some(env) = &self.styles else {
            self.desc_stack.push(NodeDesc::default());
            // A transition does not require a stylesheet to be running.
            self.apply_transition(n);
            return;
        };
        let m = self.peek(n);

        // The node's identity as a string, whichever API produced it. Focus and
        // hover are held as `StableId` (that is what `AppSnapshot` restores and
        // what the agent addresses), while a structured `NodeId` renders on
        // demand — so the comparison has to be made on the rendered form or a
        // node built with `id_at` could never be focused at all.
        let id_str: Option<String> = match (&m.id, m.node_id) {
            (Some(i), _) => Some(i.as_str().to_string()),
            (None, Some(nid)) => Some(nid.to_string_in(&self.symbols)),
            (None, None) => None,
        };

        // B.6a: interaction states carry their CSS-familiar aliases, and the
        // widget's semantic states are style-matchable too.
        let mut states = Vec::new();
        let matches_visual = |v: &Option<StableId>| matches!((id_str.as_deref(), v.as_ref()), (Some(a), Some(b)) if a == b.as_str());
        if matches_visual(&self.visual.focused) {
            states.push("focused".to_string());
            states.push("focus".to_string());
        }
        if matches_visual(&self.visual.hovered) {
            states.push("hovered".to_string());
            states.push("hover".to_string());
        }
        states.extend(m.states.iter().map(|s| s.as_str().to_string()));
        // W1: `disabled` is inherited, so a control inside a disabled
        // container matches `:disabled` too.
        if m.disabled || self.disabled_depth > 0 {
            states.push("disabled".to_string());
        }
        // Selector matching still needs strings; they are materialized here,
        // once per resolve, instead of being carried on every node all frame.
        let mut classes = m.classes.clone();
        for k in m.class_syms.iter() {
            classes.push(self.symbols.text(k).to_string());
        }
        let desc = NodeDesc {
            id: id_str,
            classes,
            states,
            ty: m.role.as_str().to_string(),
        };

        // B.2b: inside a `.container()`, container queries test that
        // ancestor's size instead of the window's.
        let media = match self.container_size() {
            Some(size) => std::borrow::Cow::Owned(MediaContext {
                container: Some(size),
                ..env.media.clone()
            }),
            None => std::borrow::Cow::Borrowed(&env.media),
        };
        let computed =
            lumen_style::resolve_with_ancestors(&env.sources, &desc, &self.desc_stack, &media);
        let mut css = Style::new();
        for (prop, c) in &computed {
            lumen_style::apply(&mut css, prop, &c.value, &env.tokens);
        }
        apply_css_paint(self.at(n), &css);
        // B.5: substitute the mid-flight blend before anything consumes the
        // style — the same point, and the same reason, as `build_node`.
        self.apply_transition(n);
        self.apply_keyframes(n, &css);
        self.pending_css.insert(n, css);
        // B.1: this node is now an ancestor for its children's matching.
        self.desc_stack.push(desc);
    }
}

// --- making the ordering unrepresentable -----------------------------------
//
// The prototype above found a real hazard: a widget that calls `resolve()`
// before declaring its id/classes is silently unstyled, and one that never
// calls it at all is silently unstyled too. Neither produces a diagnostic.
// `ProgressBar` shipped with the first bug in this very file.
//
// Comments do not fix that; types do. The node passes through two states, and
// each one exposes only the operations legal in it:
//
//   Declaring — the cascade's inputs may still be set (id, class, role,
//               states, disabled) and no child may exist yet, because this
//               node's `NodeDesc` is not on the ancestor stack.
//   Open      — the cascade has run; children may be lowered, and the
//               matchable properties can no longer be changed behind its back.
//
// `resolve` is the only way from one to the other, `end` exists only on `Open`,
// and both guards are `#[must_use]`. So "children before resolve", "declare
// after resolve" and "never resolve at all" stop being mistakes a widget author
// can make — they stop compiling.

/// A node whose cascade inputs are still being declared.
///
/// Consumed by [`resolve`](Declaring::resolve), which is the only route to a
/// node that may have children.
///
/// # The mistakes this makes impossible
///
/// Each of these was reachable before, and silently produced an unstyled node.
///
/// A child before the cascade has run — `child` does not exist on `Declaring`,
/// so the parent cannot be missing from the ancestor stack when a descendant
/// selector is matched against it:
///
/// ```compile_fail
/// # use lumen_widgets::direct::TreeSink;
/// # use lumen_core::semantics::Role;
/// # use lumen_widgets::Label;
/// let mut sink = TreeSink::new();
/// let mut d = sink.node(None, Role::Group);
/// d.child_of(Label::new("too early"));   // no method `child` on `Declaring`
/// ```
///
/// A class declared after the cascade has run — the matchable setters are gone
/// from `Open`, so a rule can no longer be defeated by ordering. This is the
/// exact bug `ProgressBar` shipped with:
///
/// ```compile_fail
/// # use lumen_widgets::direct::TreeSink;
/// # use lumen_core::semantics::Role;
/// let mut sink = TreeSink::new();
/// let open = sink.node(None, Role::Group).resolve();
/// open.class("too-late");             // no method `class` on `Open`
/// ```
///
/// Ending without resolving at all — `end` exists only on `Open`, and `Open` is
/// only reachable through `resolve`:
///
/// ```compile_fail
/// # use lumen_widgets::direct::TreeSink;
/// # use lumen_core::semantics::Role;
/// # use lumen_layout::LayoutStyle;
/// let mut sink = TreeSink::new();
/// let d = sink.node(None, Role::Group);
/// d.end(&LayoutStyle::default(), &[], false);   // no method `end` on `Declaring`
/// ```
///
/// And the correct shape, for contrast:
///
/// ```
/// # use lumen_widgets::direct::TreeSink;
/// # use lumen_core::semantics::Role;
/// # use lumen_layout::LayoutStyle;
/// # use lumen_widgets::Label;
/// let mut sink = TreeSink::new();
/// let mut open = sink.node(None, Role::Group).class("panel").resolve();
/// let child = open.child_of(Label::new("in time"));
/// open.end(&LayoutStyle::default(), &[child], false);
/// ```
#[must_use = "a declared node must be resolved and ended, or it never reaches the tree"]
pub struct Declaring<'a> {
    sink: &'a mut TreeSink,
    n: NodeIndex,
}

/// A resolved node, open for children.
#[must_use = "an open node must be ended, or its layout node is never created"]
pub struct Open<'a> {
    sink: &'a mut TreeSink,
    n: NodeIndex,
}

impl<'a> Declaring<'a> {
    /// This node's index — for tests and for wiring handlers by id.
    pub fn index(&self) -> NodeIndex {
        self.n
    }
    /// Accessible name.
    pub fn label(self, s: impl Into<String>) -> Self {
        let v = s.into();
        self.sink.label(self.n, v);
        self
    }
    /// Current value.
    pub fn value(self, s: impl Into<String>) -> Self {
        let v = s.into();
        self.sink.value(self.n, v);
        self
    }
    /// Stable id. Matchable, so it must be set before `resolve`.
    pub fn id(self, id: impl Into<StableId>) -> Self {
        let v = id.into();
        self.sink.id(self.n, v);
        self
    }
    /// A `.lss` class. Matchable, so it must be set before `resolve`.
    pub fn class(self, c: impl Into<String>) -> Self {
        let v = c.into();
        self.sink.class(self.n, v);
        self
    }
    /// A `.lss` class from an already-interned symbol.
    pub fn class_sym(self, c: Sym) -> Self {
        self.sink.class_sym(self.n, c);
        self
    }
    /// A `.lss` class, interned. The allocation-free form: a `&'static str`
    /// costs nothing after its first use anywhere in the app.
    pub fn class_static(self, c: &'static str) -> Self {
        let k = self.sink.sym(c);
        self.sink.class_sym(self.n, k);
        self
    }
    /// Structured identity — `("row", 5)` with no `format!`.
    pub fn id_at(self, name: &'static str, index: u32) -> Self {
        let k = self.sink.sym(name);
        self.sink.node_id(self.n, NodeId::at(k, index));
        self
    }
    /// Structured identity, unindexed.
    pub fn id_static(self, name: &'static str) -> Self {
        let k = self.sink.sym(name);
        self.sink.node_id(self.n, NodeId::name(k));
        self
    }
    /// Advertised actions.
    pub fn actions(self, a: Vec<Action>) -> Self {
        self.sink.actions(self.n, a);
        self
    }
    /// Semantic states. Matchable (`checkbox:checked`), so before `resolve`.
    pub fn states(self, s: Vec<SemState>) -> Self {
        self.sink.states(self.n, s);
        self
    }
    /// Keyboard focusable.
    pub fn focusable(self, yes: bool) -> Self {
        self.sink.focusable(self.n, yes);
        self
    }
    /// Elide from semantics.
    pub fn elide(self, yes: bool) -> Self {
        self.sink.elide(self.n, yes);
        self
    }
    /// Disabled. Matchable (`:disabled`), so before `resolve`.
    pub fn disabled(self, yes: bool) -> Self {
        self.sink.disabled(self.n, yes);
        self
    }
    /// Click handler.
    pub fn on_click(self, h: crate::Handler) -> Self {
        self.sink.on_click(self.n, h);
        self
    }
    /// Background fill (the sheet may still override it).
    pub fn background(self, c: Color) -> Self {
        self.sink.background(self.n, c);
        self
    }
    /// Border.
    pub fn border(self, b: Border) -> Self {
        self.sink.border(self.n, b);
        self
    }
    /// Corner radius.
    pub fn corner_radius(self, r: f64) -> Self {
        self.sink.corner_radius(self.n, r);
        self
    }
    /// Text content. Declared before `resolve` because the cascade restyles it
    /// and measurement happens afterwards.
    pub fn text(self, s: impl Into<String>, ts: TextStyle) -> Self {
        let v = s.into();
        self.sink.text(self.n, v, ts);
        self
    }
    /// Fold a [`Common`] on. Returns `Self`, so it cannot land after `resolve`.
    pub fn common(self, common: Common) -> (Self, bool) {
        let disabled = apply_common(self.sink, self.n, common);
        (self, disabled)
    }

    /// Run the cascade and open the node for children.
    ///
    /// The only transition. Everything matchable is already declared; nothing
    /// declared after this could have affected selection anyway.
    pub fn resolve(self) -> Open<'a> {
        self.sink.resolve(self.n);
        Open {
            sink: self.sink,
            n: self.n,
        }
    }
}

impl<'a> Open<'a> {
    /// This node's index.
    pub fn index(&self) -> NodeIndex {
        self.n
    }

    /// Lower a boxed child into this node.
    ///
    /// Only reachable from `Open`, so a child can never be written while its
    /// parent is missing from the ancestor stack.
    pub fn child(&mut self, w: Node) -> LayoutNode {
        let (_, ln) = w.lower(self.sink, Some(self.n));
        ln
    }

    /// [`child`](Self::child) for a widget whose type is known here — boxes it
    /// for you. The static-arity case (a composite with two named fields)
    /// should use this; a `Vec` of mixed children needs [`Node`].
    pub fn child_of<W: Direct + 'static>(&mut self, w: W) -> LayoutNode {
        self.child(node(w))
    }

    /// Lower a heterogeneous child list, returning their layout nodes in order.
    ///
    /// This is the shape every real view has — `column(vec![…])` — and the one
    /// the trait could not express before `lower` took `self: Box<Self>`.
    pub fn children(&mut self, kids: Vec<Node>) -> Vec<LayoutNode> {
        kids.into_iter().map(|k| self.child(k)).collect()
    }

    /// Begin a nested node directly, for containers that are not themselves a
    /// [`Direct`] widget.
    pub fn begin_child(&mut self, role: Role) -> Declaring<'_> {
        self.sink.node(Some(self.n), role)
    }

    /// Borrow the sink, for the few things that are neither a property nor a
    /// child (entering a disabled subtree, say).
    pub fn sink(&mut self) -> &mut TreeSink {
        self.sink
    }

    /// Close the node: fold the cascade's layout on, create the layout node.
    pub fn end(self, style: &LayoutStyle, children: &[LayoutNode], disabled: bool) -> LayoutNode {
        self.sink.end(self.n, style, children, disabled)
    }
}

impl TreeSink {
    /// Every begun node was ended.
    ///
    /// The one mistake the type states cannot catch: `#[must_use]` warns when
    /// an `Open` is dropped unused, but a warning is not a compile error
    /// everywhere. A node begun and never ended sits in the tree with no
    /// record in the side table — invisible to semantics, so invisible to the
    /// agent and to assistive tech, which is precisely the class of bug this
    /// framework cannot afford to ship. Call this at the end of a build.
    pub fn assert_balanced(&self) {
        assert!(
            self.open.is_empty(),
            "{} node(s) were begun and never ended; they are in the tree with \
             no semantics record: {:?}",
            self.open.len(),
            self.open
                .iter()
                .map(|(n, m, _)| (*n, m.role))
                .collect::<Vec<_>>()
        );
        assert!(
            self.desc_stack.is_empty(),
            "the cascade's ancestor stack is unbalanced ({} left); a resolved \
             node was not ended, so later siblings matched against a stale chain",
            self.desc_stack.len()
        );
    }

    /// Begin a node under `parent`, in the declaring state.
    ///
    /// The guarded entry point — [`begin`](TreeSink::begin) remains for the
    /// `Element` path, which is centrally ordered by its own walk.
    pub fn node(&mut self, parent: Option<NodeIndex>, role: Role) -> Declaring<'_> {
        let n = self.begin(parent, role);
        Declaring { sink: self, n }
    }
}

// --- scope memoization, without a cloneable Element ------------------------
//
// The question this answers: `cx.scope` memoization is the one part of the
// engine that genuinely *retains* an `Element` — `shared: Option<Rc<Element>>`
// on a memo-hit stub. If it cannot survive without one, direct lowering is dead
// however good its other numbers look.
//
// Reading `splice_span` settles it: **the fast path never touches `Element`.**
// A memo hit is pure tree surgery — `detach` the retained subtree from the
// parent being rebuilt and `attach_last_child` it under the new one, both O(1)
// since the child list is doubly linked. The `Rc<Element>` exists only for the
// *fallback*, when splicing is refused (the span's root died, or it contains an
// animating node whose styles are mid-interpolation).
//
// So the design question is narrower than it looked: with no `Element` to
// re-lower, the fallback has to be "run the closure again", which is what a
// cache miss already does. That is strictly more work than re-lowering a cached
// node — but only on a path that is refused, and a scope closure is pure by
// ADR-013, so re-running it is always sound.

/// One memoized scope's retained span.
#[derive(Clone, Copy)]
pub struct SpanRec {
    /// The subtree root retained from the previous frame.
    pub root: NodeIndex,
    /// The caller's dependency stamp; a change forces a rebuild.
    pub dep: u64,
    /// Preorder node count, for reporting.
    pub count: usize,
    /// The outside context the span was built in. A splice into a different
    /// one would reuse a subtree the cascade would now resolve differently.
    pub ctx: u128,
    /// The animation-registry epoch when `had_anim` was determined.
    pub anim_epoch: u64,
    /// Whether the span contained a running animation at that epoch.
    pub had_anim: bool,
}

/// What a frame did, for the benchmark to assert on.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameStats {
    /// Scopes reused by splicing.
    pub spliced: usize,
    /// Scopes whose closure ran.
    pub rebuilt: usize,
    /// Nodes reused without being rebuilt.
    pub nodes_reused: usize,
    /// Nodes freed by the sweep.
    pub nodes_freed: usize,
}

impl TreeSink {
    /// Start a new frame over the retained tree.
    ///
    /// The tree persists across frames — that is what makes splicing possible.
    /// The previous root is remembered so the sweep can free whatever this
    /// frame did not reattach.
    pub fn begin_frame(&mut self) {
        self.old_root = Some(self.tree.root());
        std::mem::swap(&mut self.prev_spans, &mut self.spans);
        self.spans.clear();
        self.stats = FrameStats::default();
    }

    /// A memoized subtree.
    ///
    /// If `key`'s span survives from the previous frame and `dep` is unchanged,
    /// the retained nodes are re-parented under `parent` and `f` never runs.
    /// Otherwise `f` runs and its span is recorded for next time.
    pub fn scope<F>(
        &mut self,
        parent: Option<NodeIndex>,
        key: u64,
        dep: u64,
        f: F,
    ) -> (NodeIndex, LayoutNode)
    where
        F: FnOnce(&mut TreeSink, Option<NodeIndex>) -> (NodeIndex, LayoutNode),
    {
        let ctx = self.ctx_hash();
        if let Some(rec) = self.prev_spans.get(&key).copied() {
            // Both must match: the scope's own data AND the surroundings that
            // feed the cascade. Checking `dep` alone reuses a span whose
            // styling would now resolve differently.
            // AN1: refuse to splice a span containing an animating node — its
            // styles are mid-interpolation, so the retained work is stale and
            // reusing it freezes the transition at this frame.
            let animated = if rec.anim_epoch == self.anim_epoch {
                // Nothing started or finished since this span was judged, so
                // the verdict still holds — no subtree walk.
                rec.had_anim
            } else {
                self.span_has_running_anim(rec.root)
            };
            if rec.dep == dep && rec.ctx == ctx && self.tree.is_alive(rec.root) && !animated {
                if let Some(raw) = self.tree.lnode(rec.root) {
                    // The whole memo hit: two pointer updates and a record.
                    self.tree.detach(rec.root);
                    match parent {
                        Some(p) => self.tree.attach_last_child(p, rec.root),
                        None => self.tree.set_root(rec.root),
                    }
                    let mut rec = rec;
                    rec.anim_epoch = self.anim_epoch;
                    rec.had_anim = false;
                    self.spans.insert(key, rec);
                    self.stats.spliced += 1;
                    self.stats.nodes_reused += rec.count;
                    return (rec.root, LayoutNode::from_raw(raw));
                }
            }
        }
        let before = self.tree.len();
        let (n, ln) = f(self, parent);
        let count = self.tree.len().saturating_sub(before);
        let had_anim = self.span_has_running_anim(n);
        self.spans.insert(
            key,
            SpanRec {
                root: n,
                dep,
                count,
                ctx,
                anim_epoch: self.anim_epoch,
                had_anim,
            },
        );
        self.stats.rebuilt += 1;
        (n, ln)
    }

    /// Free everything the frame did not reattach.
    ///
    /// Spliced spans were detached from the old parent, so they are no longer
    /// reachable from the old root — the walk enumerates only dead nodes, which
    /// is what keeps a memo-heavy frame O(changed) rather than O(tree).
    pub fn end_frame(&mut self) {
        let Some(old_root) = self.old_root.take() else {
            return;
        };
        if old_root == self.tree.root() || !self.tree.is_alive(old_root) {
            return;
        }
        let mut stack = vec![old_root];
        while let Some(n) = stack.pop() {
            if n.is_none() || n == self.tree.root() || !self.tree.is_alive(n) {
                continue;
            }
            let mut c = self.tree.first_child(n);
            while c.is_some() {
                stack.push(c);
                c = self.tree.next_sibling(c);
            }
            if let Some(raw) = self.tree.lnode(n) {
                self.layout.remove(LayoutNode::from_raw(raw));
            }
            self.meta.remove(n);
            self.pending_css.remove(&n);
            self.tree.free_one(n);
            self.stats.nodes_freed += 1;
        }
    }

    /// What the last frame did.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }
}

// --- P1: text measurement feeding layout -----------------------------------
//
// `build_node` shapes a text leaf and writes a fixed size onto the style before
// taffy sees it. Three inputs arrive at different times — the widget's own
// width, the cascade's `text-wrap`, and the content — and today they are
// reconciled by mutating one element.
//
// In the sink they meet at `end()` instead, which is the moment all three are
// known: the widget has declared its content, `resolve` has folded the sheet on,
// and the caller is handing over the style. So the reconciliation point exists
// without an element; it just moved from "the element everyone mutates" to "the
// call that closes the node".
//
// The rules are `build_node`'s, deliberately including the two it documents as
// hard-won: an explicit width or height is never overwritten by a measurement,
// and a percentage width cannot feed the wrap width (the containing block is not
// resolved until layout runs, which is after this).

impl TreeSink {
    /// Attach a text engine, enabling measurement of text leaves.
    pub fn with_text(mut self, text: lumen_text::TextEngine) -> TreeSink {
        self.text = Some(Box::new(text));
        self
    }

    /// Size a text leaf's box, mirroring `build_node`'s reconciliation.
    ///
    /// Returns the wrap width used, so a caller can assert on it.
    fn measure_text(&mut self, n: NodeIndex, style: &mut LayoutStyle) -> Option<f32> {
        let engine = self.text.as_mut()?;
        let (txt, ts) = match &self.open.iter().rposition(|(k, _, _)| *k == n) {
            Some(i) => match &self.open[*i].1.content {
                NodeContent::Text(t, ts) => (t.clone(), ts.clone()),
                _ => return None,
            },
            None => return None,
        };
        let (pl, pr) = (dim_px(style.padding.left), dim_px(style.padding.right));
        let (pt, pb) = (dim_px(style.padding.top), dim_px(style.padding.bottom));

        // An explicit pixel width turns the label into a wrapping paragraph; a
        // percentage cannot, because the containing block is not resolved yet.
        let mut wrap = match style.width {
            Dim::Px(w) => Some((w - (pl + pr) as f32).max(0.0)),
            _ => None,
        };
        // PROP1 `text-wrap: nowrap` keeps the explicit width for the BOX but
        // shapes unwrapped, so the run overflows on one line.
        if self.pending_css.get(&n).and_then(|c| c.text_wrap) == Some(false) {
            wrap = None;
        }
        let block = engine.shaped(&txt, &ts, wrap, ts.align);
        let (bw, bh) = (block.width().ceil(), block.height().ceil());
        // Never overwrite an axis the author fixed — `== Dim::Auto`, not
        // `wrap.is_none()`. Both of these guards cost real bugs to learn.
        if style.width == Dim::Auto {
            style.width = Dim::px(bw + (pl + pr) as f32);
        }
        if style.height == Dim::Auto {
            style.height = Dim::px(bh + (pt + pb) as f32);
        }
        wrap
    }
}

/// Pixels out of a `Dim`, zero for anything not definite.
fn dim_px(d: Dim) -> f64 {
    match d {
        Dim::Px(v) => v as f64,
        _ => 0.0,
    }
}

// --- P2: overlay routing, and the context a splice must match --------------
//
// The engine guards every splice with `span_ctx_hash`: the ancestor chain, the
// container size, the overlay flag and the hidden/disabled depths. All of it
// feeds the cascade, so a span may only be reused when the whole *outside
// context* is unchanged — same data under different surroundings is a different
// node.
//
// The first cut of `scope()` checked only the caller's `dep`, which is wrong in
// a way tests can demonstrate: a button retained under `.calm` and spliced under
// `.danger` keeps the styling it got under `.calm`, and one retained outside an
// overlay keeps `z = 0` after being moved into one — painting under the page it
// is supposed to float above.

/// Nodes in an overlay subtree paint in a final pass above the rest of the UI.
pub const OVERLAY_Z: u32 = 1000;

impl TreeSink {
    /// Enter an overlay subtree: this node and its descendants route to the
    /// overlay pass. Inherited, not local — a button inside a dropdown is as
    /// much part of the overlay as the dropdown.
    pub fn enter_overlay(&mut self) {
        self.overlay_depth += 1;
    }

    /// Leave an overlay subtree.
    pub fn exit_overlay(&mut self) {
        self.overlay_depth = self.overlay_depth.saturating_sub(1);
    }

    /// Whether the cursor is inside an overlay.
    pub fn in_overlay(&self) -> bool {
        self.overlay_depth > 0
    }

    /// The outside context a retained span must match to be spliceable.
    ///
    /// Mirrors `span_ctx_hash`: everything that can change what the cascade
    /// resolves for a node without the node's own data changing. `IdHasher`
    /// rather than the default: this runs once per scope per frame, and a
    /// collision splices a stale subtree — a wrong view, not a slow one.
    pub(crate) fn ctx_hash(&self) -> u128 {
        use std::hash::Hash;
        let mut h = lumen_core::identity::IdHasher::new();
        for d in &self.desc_stack {
            d.id.hash(&mut h);
            d.classes.hash(&mut h);
            d.states.hash(&mut h);
            d.ty.hash(&mut h);
        }
        self.in_overlay().hash(&mut h);
        (self.disabled_depth > 0).hash(&mut h);
        // B.2b: the enclosing container's size. A container that resized makes
        // its descendants' `@media container()` rules resolve differently with
        // no change to their own data — the P2 hazard in another guise.
        if let Some((cw, ch)) = self.container_size() {
            cw.to_bits().hash(&mut h);
            ch.to_bits().hash(&mut h);
        }
        // P8: the build generation. A tier-2 swap replaced the code that
        // produced every retained span.
        self.build_gen.hash(&mut h);
        // P5: the live stylesheet's revision. A retained span is already-styled
        // nodes, so an edit makes every one of them stale — where the Element
        // model's scope cache is pre-styling and survives a reload untouched.
        // Content-addressed, so a no-op save costs nothing.
        self.styles.as_ref().map(|e| e.gen).hash(&mut h);
        h.finish128()
    }
}

// --- P3: transitions, and their coupling to memoization --------------------
//
// `apply_transitions(&el.id, &mut css)` blends a mid-flight transition into the
// resolved style, and `splice_span` **refuses any span containing an animating
// node** because its styles are mid-interpolation. So animation and memoization
// are coupled, and getting it wrong is a silent, visual-only bug: the node
// freezes at the frame it was first spliced and never finishes its transition.
//
// The blending itself composes exactly like the cascade — it is a function from
// (id, clock) onto the resolved `Style`, and never needed an element. The part
// that needs care is the refusal.
//
// This models the transition source directly rather than reimplementing the
// `.lss` `transition:` parser: the unknown here is the *interaction* with
// splicing, not the property syntax.

/// A running transition on one node.
#[derive(Clone, Copy)]
pub struct Anim {
    /// Start colour.
    pub from: Color,
    /// End colour.
    pub to: Color,
    /// Clock reading when it began, ms.
    pub start_ms: f64,
    /// Duration, ms.
    pub dur_ms: f64,
}

impl Anim {
    /// The blended value at `now`, and whether the transition is still running.
    fn at(&self, now: f64) -> (Color, bool) {
        let t = ((now - self.start_ms) / self.dur_ms).clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * t as f32;
        (
            Color::new_linear(
                lerp(self.from.r, self.to.r),
                lerp(self.from.g, self.to.g),
                lerp(self.from.b, self.to.b),
                lerp(self.from.a, self.to.a),
            ),
            t < 1.0,
        )
    }
}

impl TreeSink {
    /// Advance the animation clock.
    pub fn set_clock(&mut self, ms: f64) {
        self.clock_ms = ms;
    }

    /// Start a background transition on the node with this id.
    pub fn start_transition(&mut self, id: impl Into<StableId>, a: Anim) {
        self.anims.insert(id.into(), a);
        self.anim_epoch += 1;
    }

    /// Whether any transition is running — the gate the engine uses so a frame
    /// with no animation never pays for the span scan.
    pub fn animating(&self) -> bool {
        !self.anims.is_empty()
    }

    /// Blend any running transition into a node's resolved paint.
    ///
    /// Called from `resolve`, where the cascade's result is still in hand —
    /// the same point `build_node` calls `apply_transitions`.
    fn apply_transition(&mut self, n: NodeIndex) {
        if self.anims.is_empty() {
            return;
        }
        let Some(id) = self.peek(n).id.clone() else {
            return;
        };
        let Some(anim) = self.anims.get(&id).copied() else {
            return;
        };
        let (c, running) = anim.at(self.clock_ms);
        self.at(n).background = Some(c);
        // The flag is introspection only — `span_has_running_anim` reads the
        // registry, for the reason documented there.
        self.at(n).animating = running;
        if !running {
            self.anims.remove(&id);
            self.anim_epoch += 1;
        }
    }

    /// Whether a retained span contains a node mid-transition.
    ///
    /// # Why this consults the registry and not a per-node flag
    ///
    /// The first cut marked nodes `animating` during `resolve` and tested that
    /// flag here. It deadlocked on itself: a node is only marked while it is
    /// being resolved, a node is only resolved if its span was *not* spliced,
    /// and the span is only refused if the node is marked. So the very first
    /// memoized frame spliced the animating node, it never resolved again, and
    /// the transition froze at frame zero — the failure this check exists to
    /// prevent, caused by the check.
    ///
    /// The engine's `span_has_running_anim` avoids it by testing the *retained
    /// meta's id* against the animation registry, which lives in engine state
    /// and is populated by whatever started the transition — never by the build.
    /// That breaks the cycle: the registry is knowable before the span is
    /// examined. This mirrors it.
    ///
    /// Gated on an animation actually running, as the engine gates it: with
    /// none — the overwhelmingly common case — a memo hit touches one node.
    fn span_has_running_anim(&self, root: NodeIndex) -> bool {
        let any_keyframes = self.keyframes_running();
        if self.anims.is_empty() && !any_keyframes {
            return false;
        }
        self.tree.subtree_preorder(root).into_iter().any(|n| {
            let Some(id) = self.meta.string_id(n) else {
                return false;
            };
            self.anims.contains_key(id) || self.key_anims.get(id).is_some_and(|(_, done)| !done)
        })
    }
}

// --- P5: hot reload --------------------------------------------------------
//
// `set_stylesheet` in the engine carries the line that makes this interesting:
//
//     // A.5b: resolution results embed the sheet — invalidate the memo
//     // (scope caches stay: cached Elements are pre-styling).
//
// In the `Element` model a memoized scope holds **unstyled** elements — the
// cascade runs later, in `build_node` — so a stylesheet edit invalidates the
// resolution cache and nothing else. Every scope stays memoized and no closure
// re-runs; the cached elements are simply re-styled on the way down.
//
// Direct lowering inverts that. A retained span is finished, already-styled
// nodes in the tree, so a sheet edit makes every span stale and there is no
// pre-styling form to re-style. **A reload frame is a full rebuild.**
//
// That is a genuine, permanent cost of this architecture and not a bug to fix
// away. What must not happen is the *silent* version: splicing across a sheet
// change and keeping the old colours, so hot reload appears to do nothing.

impl TreeSink {
    /// Swap the live stylesheet, as a hot reload does.
    ///
    /// The sheet's content hash joins the splice guard, so every retained span
    /// is invalidated by an edit — and by an edit *only*. An editor that saves
    /// an unchanged file, or a watcher that fires twice, hashes the same and
    /// costs nothing.
    pub fn set_stylesheet(&mut self, env: StyleEnv) {
        self.styles = Some(env);
    }
}

// --- Step 2: identity without allocation -----------------------------------
//
// Attribution said where the remaining per-node cost is, and it was not where
// "intern the strings" assumes:
//
//     bare node (floor)      0.09 allocs/node
//     + STATIC short id      0.09          ← the sink stores it for free
//     + format!()-minted id  2.09          ← all 2.00 is the CALLER's String
//     + one class            4.09          ← 2.00 for the class
//
// A short `StableId` inlines into its `SmolStr` and costs nothing to store, so
// interning the id table would buy exactly zero. The 2.00 allocations are
// `format!("row{i}")` at the call site — made before the sink ever sees them.
//
// So the two halves need different fixes:
//
//   * **Ids** need a *structured* form, so no string is minted at all. This is
//     the shape ADR-021 already uses for scope keys — a name and an index —
//     and it renders to `"row5"` only when something asks, which is exactly
//     when the agent or a test is looking.
//   * **Classes** genuinely do want interning: `.class("row")` allocates a
//     `String` plus the `Vec` that holds it, on every node, forever, for a
//     string that is almost always one of a handful of `&'static str`s.

/// An interned string, 4 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Sym(pub u32);

/// The interning table: `Sym` in, `&str` out.
///
/// Deliberately not a global. A sink owns its table so the framework can be
/// tuned per app — a small one keeps a tiny table, a large one can pre-size it
/// — and so nothing leaks between tests.
#[derive(Default)]
pub struct Symbols {
    by_text: HashMap<&'static str, Sym>,
    text: Vec<&'static str>,
    /// Strings that were not `'static` and had to be kept alive.
    owned: Vec<Box<str>>,
}

impl Symbols {
    /// Intern a `&'static str` — the common case, and the free one.
    pub fn intern_static(&mut self, s: &'static str) -> Sym {
        if let Some(k) = self.by_text.get(s) {
            return *k;
        }
        let k = Sym(self.text.len() as u32);
        self.text.push(s);
        self.by_text.insert(s, k);
        k
    }

    /// Intern a borrowed string, allocating once the first time it is seen.
    ///
    /// A dynamic class name still costs one allocation, but only on its *first*
    /// use — not once per node per frame, which is what `Vec<String>` did.
    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(k) = self.by_text.get(s) {
            return *k;
        }
        let boxed: Box<str> = s.into();
        // Safe: `owned` keeps the allocation alive for the table's lifetime and
        // is never mutated or shrunk, so the slice stays valid.
        let leaked: &'static str = unsafe { &*(&*boxed as *const str) };
        self.owned.push(boxed);
        let k = Sym(self.text.len() as u32);
        self.text.push(leaked);
        self.by_text.insert(leaked, k);
        k
    }

    /// The text behind a symbol.
    pub fn text(&self, s: Sym) -> &str {
        self.text.get(s.0 as usize).copied().unwrap_or("")
    }

    /// How many distinct strings are interned.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether nothing is interned.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A node's class list, inline up to three.
///
/// Interning got the *strings* to zero allocations, and then the `Vec<Sym>`
/// holding them became the whole remaining cost — one buffer per node, per
/// frame, to store a single 4-byte symbol. Real nodes carry nought to two
/// classes, so three inline covers them and the spill keeps the rare case
/// correct rather than merely fast.
#[derive(Default, Clone)]
pub struct ClassSet {
    inline: [Sym; 3],
    len: u8,
    spill: Option<Vec<Sym>>,
}

impl ClassSet {
    /// Add a class.
    pub fn push(&mut self, s: Sym) {
        if (self.len as usize) < self.inline.len() {
            self.inline[self.len as usize] = s;
            self.len += 1;
        } else {
            self.spill.get_or_insert_with(Vec::new).push(s);
        }
    }

    /// Iterate every class, inline then spilled.
    pub fn iter(&self) -> impl Iterator<Item = Sym> + '_ {
        self.inline[..self.len as usize]
            .iter()
            .copied()
            .chain(self.spill.iter().flat_map(|v| v.iter().copied()))
    }

    /// How many classes.
    pub fn len(&self) -> usize {
        self.len as usize + self.spill.as_ref().map_or(0, |v| v.len())
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Sym {
    fn default() -> Sym {
        Sym(u32::MAX)
    }
}

/// A node's identity, 8 bytes and no allocation.
///
/// A name plus an optional index, so `("row", 5)` needs no `format!`. The
/// string form is produced on demand — which is when a test, a selector or the
/// agent asks, not on every node of every frame.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId {
    /// The interned name.
    pub name: Sym,
    /// The index, or `u32::MAX` for a bare name.
    pub index: u32,
}

impl NodeId {
    /// A bare name, no index.
    pub const NONE_INDEX: u32 = u32::MAX;

    /// `name`, unindexed.
    pub fn name(name: Sym) -> NodeId {
        NodeId {
            name,
            index: Self::NONE_INDEX,
        }
    }

    /// `name` at `index` — the `("row", 5)` shape, with no string minted.
    pub fn at(name: Sym, index: u32) -> NodeId {
        NodeId { name, index }
    }

    /// Render to the string form a selector or the agent expects.
    pub fn to_string_in(&self, syms: &Symbols) -> String {
        let base = syms.text(self.name);
        if self.index == Self::NONE_INDEX {
            base.to_string()
        } else {
            format!("{base}{}", self.index)
        }
    }
}

// --- Step 3: the side table, columnar --------------------------------------
//
// `Element` was 1072 bytes of uniform record per node, and removing it was the
// point of this whole exercise. `Meta` is 656 bytes of uniform record per node,
// held in a `HashMap<NodeIndex, Meta>`. It is the same shape of problem moved
// one layer down, and it is now the largest single per-node cost left.
//
// Two costs, not one:
//
//   * **Hashing.** Every property read hashes a `NodeIndex`. The semantics walk
//     the agent performs does this per node per field.
//   * **Uniformity.** A node pays for `caret_byte`, `selection`, twelve handler
//     slots and a `label` `String` whether or not it is a text field. Almost
//     none of them are ever set.
//
// Columns fix both. A dense `Vec<T>` indexed by the node's arena index is a
// bounds-checked array read, and a column nobody touches is never grown — the
// framework can size itself to what an app actually uses, which is the tuning
// the project is after.

/// The per-node side table, stored as columns rather than records.
///
/// Indexed densely by `NodeIndex::index()`, so a lookup is an array read.
#[derive(Default)]
pub struct MetaStore {
    /// Highest slot in use, so a scan knows where to stop.
    len: usize,
    /// Generation per slot, so a stale `NodeIndex` reads as absent rather than
    /// as whatever now occupies its slot.
    generation: Vec<u32>,
    /// Whether the slot holds a live record.
    live: Vec<bool>,

    // --- hot: read for every node, every frame ---
    role: Vec<Role>,
    node_id: Vec<Option<NodeId>>,
    class_syms: Vec<ClassSet>,
    background: Vec<Option<Color>>,
    corner_radius: Vec<f32>,
    layout_style: Vec<CompactStyle>,
    flags: Vec<MetaFlags>,

    // --- cold: set by a minority of nodes ---
    /// Everything rare, allocated only for the nodes that use it.
    cold: HashMap<u32, Box<ColdMeta>>,
}

/// The booleans, packed into one byte instead of five.
///
/// Hand-rolled rather than pulling in `bitflags`: four flags do not justify a
/// dependency, and ADR-003 keeps that list deliberately short.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MetaFlags(u8);

impl MetaFlags {
    /// Keyboard focusable.
    pub const FOCUSABLE: MetaFlags = MetaFlags(1 << 0);
    /// Elided from semantics.
    pub const ELIDE: MetaFlags = MetaFlags(1 << 1);
    /// Disabled.
    pub const DISABLED: MetaFlags = MetaFlags(1 << 2);
    /// Mid-transition.
    pub const ANIMATING: MetaFlags = MetaFlags(1 << 3);

    /// No flags.
    pub const fn empty() -> MetaFlags {
        MetaFlags(0)
    }
    /// Whether every bit in `f` is set.
    pub const fn contains(self, f: MetaFlags) -> bool {
        self.0 & f.0 == f.0
    }
    /// Set or clear `f`.
    pub fn set(&mut self, f: MetaFlags, on: bool) {
        if on {
            self.0 |= f.0;
        } else {
            self.0 &= !f.0;
        }
    }
}

/// The rarely-set half of a node's record.
#[derive(Default)]
pub struct ColdMeta {
    /// String id, when the author used the string API rather than `id_at`.
    pub id: Option<StableId>,
    /// Accessible name.
    pub label: String,
    /// Current value.
    pub value: Option<String>,
    /// String classes.
    pub classes: Vec<String>,
    /// Advertised actions.
    pub actions: Vec<Action>,
    /// Semantic states.
    pub states: Vec<SemState>,
    /// Click handler.
    pub on_click: Option<crate::Handler>,
    /// Border.
    pub border: Option<Border>,
    /// Leaf content.
    pub content: NodeContent,
}

impl MetaStore {
    /// Grow the columns to cover `slot`.
    fn reserve(&mut self, slot: usize) {
        if slot < self.generation.len() {
            return;
        }
        let n = slot + 1;
        self.generation.resize(n, 0);
        self.live.resize(n, false);
        self.role.resize(n, Role::Generic);
        self.node_id.resize(n, None);
        self.class_syms.resize(n, ClassSet::default());
        self.background.resize(n, None);
        self.corner_radius.resize(n, 0.0);
        self.layout_style.resize(n, CompactStyle::default());
        self.flags.resize(n, MetaFlags::empty());
    }

    /// Start a record for `n`.
    pub fn insert(&mut self, n: NodeIndex, role: Role) {
        let i = n.index() as usize;
        self.reserve(i);
        self.generation[i] = n.generation();
        self.live[i] = true;
        self.role[i] = role;
        self.node_id[i] = None;
        self.class_syms[i] = ClassSet::default();
        self.background[i] = None;
        self.corner_radius[i] = 0.0;
        self.layout_style[i] = CompactStyle::default();
        self.flags[i] = MetaFlags::empty();
        self.cold.remove(&(i as u32));
        self.len = self.len.max(i + 1);
    }

    /// Whether `n`'s record is live *and* current — a stale index reads absent.
    pub fn contains(&self, n: NodeIndex) -> bool {
        let i = n.index() as usize;
        i < self.live.len() && self.live[i] && self.generation[i] == n.generation()
    }

    /// Drop `n`'s record.
    pub fn remove(&mut self, n: NodeIndex) {
        let i = n.index() as usize;
        if i < self.live.len() {
            self.live[i] = false;
            self.cold.remove(&(i as u32));
        }
    }

    /// The node's role.
    pub fn role(&self, n: NodeIndex) -> Role {
        self.role[n.index() as usize]
    }
    /// The node's structured id.
    pub fn node_id(&self, n: NodeIndex) -> Option<NodeId> {
        self.node_id[n.index() as usize]
    }
    /// The node's background.
    pub fn background(&self, n: NodeIndex) -> Option<Color> {
        self.background[n.index() as usize]
    }
    /// The node's corner radius.
    pub fn corner_radius(&self, n: NodeIndex) -> f64 {
        self.corner_radius[n.index() as usize] as f64
    }
    /// The compact style as stored.
    pub fn style(&self, n: NodeIndex) -> &CompactStyle {
        &self.layout_style[n.index() as usize]
    }
    /// The full style, materialized. Transient — one on the stack at a time,
    /// which is the trade the split makes.
    pub fn layout_style(&self, n: NodeIndex) -> LayoutStyle {
        self.layout_style[n.index() as usize].to_layout()
    }
    /// The packed booleans.
    pub fn flags(&self, n: NodeIndex) -> MetaFlags {
        self.flags[n.index() as usize]
    }
    /// The interned classes.
    pub fn class_syms(&self, n: NodeIndex) -> &ClassSet {
        &self.class_syms[n.index() as usize]
    }
    /// The rare half, if this node has any.
    pub fn cold(&self, n: NodeIndex) -> Option<&ColdMeta> {
        self.cold.get(&n.index()).map(|b| &**b)
    }
    /// Set the corner radius.
    pub fn set_corner_radius(&mut self, n: NodeIndex, r: f64) {
        self.corner_radius[n.index() as usize] = r as f32;
    }
    /// Set or clear a flag.
    pub fn set_flags(&mut self, n: NodeIndex, f: MetaFlags, on: bool) {
        self.flags[n.index() as usize].set(f, on);
    }
    /// Set the background.
    pub fn set_background(&mut self, n: NodeIndex, c: Option<Color>) {
        self.background[n.index() as usize] = c;
    }
    /// Set the structured id.
    pub fn set_node_id(&mut self, n: NodeIndex, id: NodeId) {
        self.node_id[n.index() as usize] = Some(id);
    }
    /// Append an interned class.
    pub fn push_class(&mut self, n: NodeIndex, c: Sym) {
        self.class_syms[n.index() as usize].push(c);
    }
    /// Set the style handed to taffy.
    pub fn set_layout_style(&mut self, n: NodeIndex, s: LayoutStyle) {
        self.layout_style[n.index() as usize] = CompactStyle::from_layout(&s);
    }

    /// The rare half, created on first use.
    pub fn cold_mut(&mut self, n: NodeIndex) -> &mut ColdMeta {
        self.cold.entry(n.index()).or_default()
    }

    // --- cold-half accessors, so the store is a complete replacement --------
    //
    // Each reads through the per-node `ColdMeta` when there is one and returns
    // the empty value otherwise, so a caller never has to know whether a node
    // happened to allocate its rare half.

    /// Accessible name.
    pub fn label(&self, n: NodeIndex) -> &str {
        self.cold(n).map(|c| c.label.as_str()).unwrap_or("")
    }
    /// Current value.
    pub fn value(&self, n: NodeIndex) -> Option<&str> {
        self.cold(n).and_then(|c| c.value.as_deref())
    }
    /// String classes (the non-interned form).
    pub fn classes(&self, n: NodeIndex) -> &[String] {
        self.cold(n).map(|c| c.classes.as_slice()).unwrap_or(&[])
    }
    /// Advertised actions.
    pub fn actions(&self, n: NodeIndex) -> &[Action] {
        self.cold(n).map(|c| c.actions.as_slice()).unwrap_or(&[])
    }
    /// Semantic states.
    pub fn states(&self, n: NodeIndex) -> &[SemState] {
        self.cold(n).map(|c| c.states.as_slice()).unwrap_or(&[])
    }
    /// Click handler.
    pub fn on_click(&self, n: NodeIndex) -> Option<&crate::Handler> {
        self.cold(n).and_then(|c| c.on_click.as_ref())
    }
    /// Border.
    pub fn border(&self, n: NodeIndex) -> Option<Border> {
        self.cold(n).and_then(|c| c.border)
    }
    /// Leaf content, or `None` for a node that is not a leaf.
    ///
    /// Returns an `Option` rather than a borrowed `NodeContent::None`: the
    /// variant holds `Rc`s, so it is not `Sync` and cannot be a `static`.
    pub fn content(&self, n: NodeIndex) -> Option<&NodeContent> {
        self.cold(n)
            .map(|c| &c.content)
            .filter(|c| !matches!(c, NodeContent::None))
    }
    /// The string id, when the author used the string API.
    pub fn string_id(&self, n: NodeIndex) -> Option<&StableId> {
        self.cold(n).and_then(|c| c.id.as_ref())
    }

    /// The node's identity as a string, whichever API produced it.
    ///
    /// The structured form is rendered here rather than being carried — which
    /// is the point of `NodeId`: the cost lands on whoever asks, and the only
    /// things that ask are selectors, tests and the agent.
    pub fn id_string(&self, n: NodeIndex, syms: &Symbols) -> Option<String> {
        if let Some(id) = self.string_id(n) {
            return Some(id.as_str().to_string());
        }
        self.node_id(n).map(|i| i.to_string_in(syms))
    }

    /// Store a finished in-flight record.
    ///
    /// Hot fields go to the columns; the rare half is allocated **only** if the
    /// node actually set something in it, which is what keeps a plain layout box
    /// from paying for a text field's twelve handler slots.
    pub fn commit(&mut self, n: NodeIndex, m: Meta) {
        let i = n.index() as usize;
        self.reserve(i);
        self.generation[i] = n.generation();
        self.live[i] = true;
        self.len = self.len.max(i + 1);

        self.role[i] = m.role;
        self.node_id[i] = m.node_id;
        self.class_syms[i] = m.class_syms;
        self.background[i] = m.background;
        self.corner_radius[i] = m.corner_radius as f32;
        self.layout_style[i] = CompactStyle::from_layout(&m.layout_style);
        let mut f = MetaFlags::empty();
        f.set(MetaFlags::FOCUSABLE, m.focusable);
        f.set(MetaFlags::ELIDE, m.elide);
        f.set(MetaFlags::DISABLED, m.disabled);
        f.set(MetaFlags::ANIMATING, m.animating);
        self.flags[i] = f;

        let needs_cold = m.id.is_some()
            || !m.label.is_empty()
            || m.value.is_some()
            || !m.classes.is_empty()
            || !m.actions.is_empty()
            || !m.states.is_empty()
            || m.on_click.is_some()
            || m.border.is_some()
            || !matches!(m.content, NodeContent::None);
        if needs_cold {
            self.cold.insert(
                i as u32,
                Box::new(ColdMeta {
                    id: m.id,
                    label: m.label,
                    value: m.value,
                    classes: m.classes,
                    actions: m.actions,
                    states: m.states,
                    on_click: m.on_click,
                    border: m.border,
                    content: m.content,
                }),
            );
        } else {
            self.cold.remove(&(i as u32));
        }
    }

    /// Every live node, in slot order — the shape a semantics walk wants.
    pub fn iter_live(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.len).filter(move |i| self.live[*i])
    }

    /// How many slots the columns cover.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// How many nodes carry a cold record.
    pub fn cold_count(&self) -> usize {
        self.cold.len()
    }

    /// Bytes the columns occupy, for measurement.
    pub fn column_bytes(&self) -> usize {
        use std::mem::size_of;
        self.generation.capacity() * size_of::<u32>()
            + self.live.capacity()
            + self.role.capacity() * size_of::<Role>()
            + self.node_id.capacity() * size_of::<Option<NodeId>>()
            + self.class_syms.capacity() * size_of::<ClassSet>()
            + self.background.capacity() * size_of::<Option<Color>>()
            + self.corner_radius.capacity() * size_of::<f32>()
            + self.layout_style.capacity() * size_of::<CompactStyle>()
            + self.flags.capacity() * size_of::<MetaFlags>()
    }

    /// Per-node bytes in the hot columns.
    pub fn hot_bytes_per_node() -> usize {
        use std::mem::size_of;
        size_of::<u32>()
            + 1
            + size_of::<Role>()
            + size_of::<Option<NodeId>>()
            + size_of::<ClassSet>()
            + size_of::<Option<Color>>()
            + size_of::<f32>()
            + size_of::<CompactStyle>()
            + size_of::<MetaFlags>()
    }
}

// --- Step 4: LayoutStyle, split by measured occupancy ----------------------
//
// The third uniform record in a row, and the dominant column at 256 of the 339
// bytes a node costs. Element 1072 -> Meta 656 -> LayoutStyle 256: the same
// habit one layer down each time, so the same fix applies — but only after
// measuring, because step 2 showed what guessing the split costs.
//
// Occupancy over 1801 real nodes:
//
//     padding          44.4%      width/height/gaps  22.2%
//     flex_direction   11.2%      align_items        11.1%
//     ...and TWENTY fields set by 0.0% of them, including every grid field,
//        margin, inset, and all four min/max dimensions.
//
// So the layout knobs a node actually turns are a handful of small ones, and
// the bulk of the record is grid tracks and box offsets that a typical node
// never touches. `margin` and `inset` alone are 64 bytes of the 256.
//
// The cold set is chosen a little more conservatively than the measurement
// alone would justify: `position`, `flex_grow` and `justify_content` measured
// 0% here but are obviously used by absolute overlays, spacers and centred
// rows in apps this probe does not model, and they are 1-4 bytes each — too
// small to be worth a pointer chase. What moves out is what is both large and
// structurally rare: grid, the two `Edges` a normal flow node never sets, the
// min/max box, and the aspect ratio.

/// The rarely-set half of a layout style — 168 bytes that most nodes skip.
#[derive(Clone, PartialEq)]
pub struct RareStyle {
    /// Outer offsets.
    pub margin: Edges,
    /// Absolute-positioning offsets.
    pub inset: Edges,
    /// Minimum width.
    pub min_width: Dim,
    /// Minimum height.
    pub min_height: Dim,
    /// Maximum width.
    pub max_width: Dim,
    /// Maximum height.
    pub max_height: Dim,
    /// Width / height ratio.
    pub aspect_ratio: Option<f32>,
    /// Grid column tracks.
    pub grid_template_columns: Vec<lumen_layout::GridTrack>,
    /// Grid row tracks.
    pub grid_template_rows: Vec<lumen_layout::GridTrack>,
    /// Grid column placement.
    pub grid_column: (lumen_layout::GridLine, lumen_layout::GridLine),
    /// Grid row placement.
    pub grid_row: (lumen_layout::GridLine, lumen_layout::GridLine),
}

impl Default for RareStyle {
    fn default() -> RareStyle {
        // Taken from `LayoutStyle`'s own default rather than restated, so the
        // two cannot drift — `Dim` has no `Default` on purpose, since `Auto`
        // and `Px(0)` are both defensible and the layout crate picks per field.
        let d = LayoutStyle::default();
        RareStyle {
            margin: d.margin,
            inset: d.inset,
            min_width: d.min_width,
            min_height: d.min_height,
            max_width: d.max_width,
            max_height: d.max_height,
            aspect_ratio: d.aspect_ratio,
            grid_template_columns: d.grid_template_columns,
            grid_template_rows: d.grid_template_rows,
            grid_column: d.grid_column,
            grid_row: d.grid_row,
        }
    }
}

/// A node's layout style, with the rare bulk behind a pointer.
#[derive(Clone)]
pub struct CompactStyle {
    /// Display mode.
    pub display: Display,
    /// Positioning scheme.
    pub position: lumen_layout::Position,
    /// Flex main-axis direction.
    pub flex_direction: FlexDirection,
    /// Flex wrapping.
    pub flex_wrap: lumen_layout::FlexWrap,
    /// Flex grow factor.
    pub flex_grow: f32,
    /// Flex shrink factor.
    pub flex_shrink: f32,
    /// Flex basis.
    pub flex_basis: Dim,
    /// Cross-axis item alignment.
    pub align_items: Option<Align>,
    /// Per-item cross-axis override.
    pub align_self: Option<Align>,
    /// Multi-line cross-axis alignment.
    pub align_content: Option<Align>,
    /// Main-axis distribution.
    pub justify_content: Option<Align>,
    /// Row gap.
    pub row_gap: Dim,
    /// Column gap.
    pub column_gap: Dim,
    /// Width.
    pub width: Dim,
    /// Height.
    pub height: Dim,
    /// Inner offsets — the most-set field of all, at 44%.
    pub padding: Edges,
    /// The rare bulk, allocated only when something in it is set.
    pub rare: Option<Box<RareStyle>>,
}

impl Default for CompactStyle {
    fn default() -> CompactStyle {
        CompactStyle::from_layout(&LayoutStyle::default())
    }
}

impl CompactStyle {
    /// Compact a `LayoutStyle`, allocating the rare half only if it is used.
    pub fn from_layout(s: &LayoutStyle) -> CompactStyle {
        let d = LayoutStyle::default();
        let needs_rare = s.margin != d.margin
            || s.inset != d.inset
            || s.min_width != d.min_width
            || s.min_height != d.min_height
            || s.max_width != d.max_width
            || s.max_height != d.max_height
            || s.aspect_ratio != d.aspect_ratio
            || !s.grid_template_columns.is_empty()
            || !s.grid_template_rows.is_empty()
            || s.grid_column != d.grid_column
            || s.grid_row != d.grid_row;
        CompactStyle {
            display: s.display,
            position: s.position,
            flex_direction: s.flex_direction,
            flex_wrap: s.flex_wrap,
            flex_grow: s.flex_grow,
            flex_shrink: s.flex_shrink,
            flex_basis: s.flex_basis,
            align_items: s.align_items,
            align_self: s.align_self,
            align_content: s.align_content,
            justify_content: s.justify_content,
            row_gap: s.row_gap,
            column_gap: s.column_gap,
            width: s.width,
            height: s.height,
            padding: s.padding,
            rare: needs_rare.then(|| {
                Box::new(RareStyle {
                    margin: s.margin,
                    inset: s.inset,
                    min_width: s.min_width,
                    min_height: s.min_height,
                    max_width: s.max_width,
                    max_height: s.max_height,
                    aspect_ratio: s.aspect_ratio,
                    grid_template_columns: s.grid_template_columns.clone(),
                    grid_template_rows: s.grid_template_rows.clone(),
                    grid_column: s.grid_column,
                    grid_row: s.grid_row,
                })
            }),
        }
    }

    /// Materialize the full style taffy consumes.
    ///
    /// Transient — one on the stack at a time, not one per node retained. That
    /// is the whole trade: the bulk exists while a node is being laid out and
    /// not for the rest of the frame.
    pub fn to_layout(&self) -> LayoutStyle {
        let d = LayoutStyle::default();
        let r = self.rare.as_deref();
        LayoutStyle {
            display: self.display,
            position: self.position,
            flex_direction: self.flex_direction,
            flex_wrap: self.flex_wrap,
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: self.flex_basis,
            align_items: self.align_items,
            align_self: self.align_self,
            align_content: self.align_content,
            justify_content: self.justify_content,
            row_gap: self.row_gap,
            column_gap: self.column_gap,
            width: self.width,
            height: self.height,
            padding: self.padding,
            margin: r.map_or(d.margin, |r| r.margin),
            inset: r.map_or(d.inset, |r| r.inset),
            min_width: r.map_or(d.min_width, |r| r.min_width),
            min_height: r.map_or(d.min_height, |r| r.min_height),
            max_width: r.map_or(d.max_width, |r| r.max_width),
            max_height: r.map_or(d.max_height, |r| r.max_height),
            aspect_ratio: r.and_then(|r| r.aspect_ratio),
            grid_template_columns: r
                .map(|r| r.grid_template_columns.clone())
                .unwrap_or_default(),
            grid_template_rows: r.map(|r| r.grid_template_rows.clone()).unwrap_or_default(),
            grid_column: r.map_or(d.grid_column, |r| r.grid_column),
            grid_row: r.map_or(d.grid_row, |r| r.grid_row),
        }
    }
}

// --- P6: @keyframes --------------------------------------------------------
//
// The architectural question was already settled by the transition prototype:
// animation state must live in a registry keyed independently of the build, or
// the refusal that prevents a frozen animation causes one. `key_anims` has the
// same shape as `anims`, so that lesson transfers unchanged.
//
// What is new is arithmetic and lifetime:
//
//   * **Multi-stop interpolation.** A transition is one `from -> to`; a
//     timeline is N stops and the phase must land between the *bracketing*
//     pair, not the endpoints.
//   * **Iteration.** Delay before the first frame, `fract()` for looping,
//     a finite `count` that ends and latches, and `alternate` reversing every
//     other pass.
//   * **Collection.** A timeline whose node vanished must not leak; the engine
//     does `key_anims.retain(|id, _| live.contains(id))` after each build.
//
// And one consequence that transitions never had, because a transition always
// ends: **an infinite timeline never finishes**, so a span containing one is
// refused forever. `Spinner` and `Skeleton` both animate continuously, so a
// loading screen is exactly the case where memoization would quietly stop
// working. That is the thing this prototype exists to measure.

/// One `@keyframes` stop's paint values.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct KeyStop {
    /// `background` at this stop.
    pub background: Option<Color>,
    /// `color` at this stop.
    pub color: Option<Color>,
    /// `opacity` at this stop.
    pub opacity: Option<f32>,
    /// `border-radius` at this stop.
    pub border_radius: Option<f32>,
}

/// A parsed timeline: stops sorted by percentage.
pub type Timeline = Vec<(f32, KeyStop)>;

/// Blend two stops.
fn lerp_stop(a: &KeyStop, b: &KeyStop, t: f32) -> KeyStop {
    let lc = |x: Option<Color>, y: Option<Color>| match (x, y) {
        (Some(x), Some(y)) => Some(Color::new_linear(
            x.r + (y.r - x.r) * t,
            x.g + (y.g - x.g) * t,
            x.b + (y.b - x.b) * t,
            x.a + (y.a - x.a) * t,
        )),
        // A property present at only one end holds that value rather than
        // snapping to nothing — the same rule the engine's stops follow.
        (Some(x), None) => Some(x),
        (None, y) => y,
    };
    let lf = |x: Option<f32>, y: Option<f32>| match (x, y) {
        (Some(x), Some(y)) => Some(x + (y - x) * t),
        (Some(x), None) => Some(x),
        (None, y) => y,
    };
    KeyStop {
        background: lc(a.background, b.background),
        color: lc(a.color, b.color),
        opacity: lf(a.opacity, b.opacity),
        border_radius: lf(a.border_radius, b.border_radius),
    }
}

/// The stop pair bracketing `phase`, blended.
///
/// Separate from the scheduling so it can be tested on its own — the bracketing
/// is where an off-by-one silently produces a plausible-but-wrong colour.
pub fn sample_timeline(stops: &Timeline, phase: f32) -> KeyStop {
    if stops.is_empty() {
        return KeyStop::default();
    }
    let phase = phase.clamp(0.0, 1.0);
    if phase <= stops[0].0 {
        return stops[0].1;
    }
    if phase >= stops[stops.len() - 1].0 {
        return stops[stops.len() - 1].1;
    }
    let i = stops.partition_point(|(p, _)| *p <= phase).max(1) - 1;
    let (p0, a) = &stops[i];
    let (p1, b) = &stops[i + 1];
    let span = (p1 - p0).max(f32::EPSILON);
    lerp_stop(a, b, (phase - p0) / span)
}

impl TreeSink {
    /// Register a timeline the sheet declared.
    pub fn add_keyframes(&mut self, name: &str, stops: Timeline) {
        let mut stops = stops;
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.keyframes.insert(name.to_string(), stops);
    }

    /// Suppress animation for users who asked for less motion.
    pub fn set_reduced_motion(&mut self, on: bool) {
        self.reduced_motion = on;
    }

    /// Whether any timeline is still running.
    pub fn keyframes_running(&self) -> bool {
        self.key_anims.values().any(|(_, done)| !done)
    }

    /// Play a node's `animation:` timeline into its resolved paint.
    ///
    /// Called from `resolve`, after the cascade and the transition blend — the
    /// same order `build_node` uses.
    fn apply_keyframes(&mut self, n: NodeIndex, css: &Style) {
        let Some(spec) = css.animation.clone() else {
            return;
        };
        if self.reduced_motion && !css.animation_force {
            return;
        }
        let Some(id) = self.peek(n).id.clone() else {
            return;
        };
        let Some(stops) = self.keyframes.get(&spec.name).cloned() else {
            return;
        };
        if stops.is_empty() {
            return;
        }

        let now = self.clock_ms;
        let fresh = !self.key_anims.contains_key(&id);
        let entry = self.key_anims.entry(id).or_insert((now, false));
        let elapsed = now - entry.0 - spec.delay_ms as f64;
        if elapsed < 0.0 {
            return; // still in the delay
        }
        let iter = elapsed / spec.duration_ms.max(1.0) as f64;
        let mut phase = iter.fract() as f32;
        if let Some(count) = spec.count {
            if iter >= count as f64 && !entry.1 {
                // Finite timelines latch on their last stop rather than
                // snapping back, and stop refusing their span.
                entry.1 = true;
                phase = 1.0;
                self.anim_epoch += 1;
            } else if entry.1 {
                phase = 1.0;
            }
        }
        if fresh {
            self.anim_epoch += 1;
        }
        if spec.alternate && (iter as u64) % 2 == 1 {
            phase = 1.0 - phase;
        }

        let stop = sample_timeline(&stops, phase);
        let m = self.at(n);
        if let Some(c) = stop.background {
            m.background = Some(c);
        }
        if let Some(r) = stop.border_radius {
            m.corner_radius = r as f64;
        }
        if let Some(c) = stop.color {
            if let NodeContent::Text(_, ts) = &mut m.content {
                ts.color = c;
            }
        }
    }

    /// Drop timelines whose nodes are no longer in the view.
    ///
    /// Without this an app that churns animated nodes leaks a registry entry
    /// per node, forever — and every one of them keeps refusing splices.
    pub fn collect_animations(&mut self) {
        if self.key_anims.is_empty() {
            return;
        }
        let mut live: HashMap<StableId, ()> = HashMap::new();
        for n in self.tree.iter_live() {
            if let Some(id) = self.meta.string_id(n) {
                live.insert(id.clone(), ());
            }
        }
        let before = self.key_anims.len();
        self.key_anims.retain(|id, _| live.contains_key(id));
        if self.key_anims.len() != before {
            self.anim_epoch += 1;
        }
    }
}

// --- P7: container queries -------------------------------------------------
//
// `@media container(...)` tests the nearest enclosing `.container()` node's
// size rather than the window's. Two things make it awkward, and both are
// about *when* the size is known:
//
//   * The size comes from the **previous** layout, because this node is being
//     built and has not been laid out yet. On the first pass there is none, and
//     queries fail closed.
//   * It feeds `span_ctx_hash`. A container that resized changes what its
//     descendants' rules resolve to **without any of their own data changing**
//     — so a memo hit inside a resized container is stale in exactly the way
//     P2's ancestor-class case was.
//
// The sink therefore has to carry a container stack, seed it from the previous
// frame's laid-out sizes, and fold the current entry into the context hash.

// --- P8: code hot reload ---------------------------------------------------
//
// A tier-2 swap replaces a component's `build()` in place; host state survives
// and an ABI-incompatible component falls back to tier 3. For the sink the
// consequence is the same as a stylesheet edit: every span that component
// produced was built by code that no longer exists, so all of them are stale.
// One generation counter covers it, exactly as `StyleEnv::gen` does.

impl TreeSink {
    /// Enter a `.container()` subtree, seeding the query size from the previous
    /// frame's layout.
    ///
    /// `seq` is the container's index in build order, which is how the engine
    /// pairs a container with its own size across frames — node indices are not
    /// stable, but build order is.
    pub fn enter_container(&mut self, n: NodeIndex, seq: usize) {
        self.container_nodes.push(n);
        self.container_stack
            .push(self.container_prev.get(seq).copied());
    }

    /// Leave a container subtree.
    pub fn exit_container(&mut self) {
        self.container_stack.pop();
    }

    /// The query size in force, if inside a container that has been laid out.
    pub fn container_size(&self) -> Option<(f64, f64)> {
        self.container_stack.last().copied().flatten()
    }

    /// Record this frame's container sizes, for the next frame's queries.
    ///
    /// Called after layout. Until it has run once, queries fail closed — which
    /// is the correct answer, not a missing feature: a container's size is not
    /// knowable while the thing inside it is still being built.
    pub fn record_container_sizes(&mut self, sizes: Vec<(f64, f64)>) {
        self.container_prev = sizes;
        self.container_nodes.clear();
    }

    /// The containers built this frame, in build order.
    pub fn container_nodes(&self) -> &[NodeIndex] {
        &self.container_nodes
    }

    /// Bump the build generation — a tier-2 code swap replaced a `build()`.
    ///
    /// Every retained span was produced by code that no longer exists, so all
    /// of them must be rebuilt. Same shape as a stylesheet edit, and for the
    /// same reason: the memo holds *output*, and the thing that produced it
    /// changed.
    pub fn set_build_generation(&mut self, gen: u64) {
        self.build_gen = gen;
    }
}
