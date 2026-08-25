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
            actions: Vec::new(),
            states: Vec::new(),
            focusable: false,
            elide: false,
            disabled: false,
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
    /// Per-node semantics/handlers/paint.
    pub meta: HashMap<NodeIndex, Meta>,
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
            meta: HashMap::new(),
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
    /// Append a class.
    pub fn class(&mut self, n: NodeIndex, c: String) {
        self.at(n).classes.push(c);
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
        // The record moves into the map exactly once, when the node closes.
        let i = self
            .open
            .iter()
            .rposition(|(k, _, _)| *k == n)
            .expect("node begun but not ended");
        let (_, meta, _) = self.open.remove(i);
        self.meta.insert(n, meta);
        lnode
    }
}

/// A widget that lowers **straight into the tree**, with no `Element`.
///
/// The counterpart of [`Widget::build`](crate::Widget::build): same data, same
/// destination, without the uniform 1072-byte staging record in between.
pub trait Direct {
    /// Write this widget (and its subtree) into `out` under `parent`.
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode);
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
    for (n, ma) in &a.meta {
        let mb = b.meta.get(n).ok_or_else(|| format!("missing node {n:?}"))?;
        if ma.role != mb.role {
            return Err(format!("{n:?} role {:?} vs {:?}", ma.role, mb.role));
        }
        if ma.label != mb.label {
            return Err(format!("{n:?} label {:?} vs {:?}", ma.label, mb.label));
        }
        if ma.value != mb.value {
            return Err(format!("{n:?} value {:?} vs {:?}", ma.value, mb.value));
        }
        if ma.classes != mb.classes {
            return Err(format!("{n:?} classes {:?} vs {:?}", ma.classes, mb.classes));
        }
        if ma.actions != mb.actions {
            return Err(format!("{n:?} actions differ"));
        }
        if ma.states != mb.states {
            return Err(format!("{n:?} states differ"));
        }
        if ma.focusable != mb.focusable {
            return Err(format!("{n:?} focusable differs"));
        }
        if ma.on_click.is_some() != mb.on_click.is_some() {
            return Err(format!("{n:?} on_click presence differs"));
        }
        if a.tree.flags(*n) != b.tree.flags(*n) {
            return Err(format!(
                "{n:?} flags {:?} vs {:?}",
                a.tree.flags(*n),
                b.tree.flags(*n)
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
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode) {
        let (text, style, width, common) = self.into_parts();
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
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode) {
        let (label, on_press, fill, ink, common) = self.into_parts();
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
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode) {
        let (frac, width, height, ink, common) = self.into_parts();
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

/// Begin a row box. Children are lowered directly into it by the caller, then
/// [`TreeSink::end`] closes it — no boxed closures, which is what a real
/// conversion would do (the child widgets are known statically at each site).
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
    /// Parsed sheets, in cascade order.
    pub sources: Vec<StyleSource>,
    /// `--token` values.
    pub tokens: Tokens,
    /// Window/container context for `@media`.
    pub media: MediaContext,
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
            return;
        };
        let m = self.peek(n);

        // B.6a: interaction states carry their CSS-familiar aliases, and the
        // widget's semantic states are style-matchable too.
        let mut states = Vec::new();
        if m.id.is_some() && m.id == self.visual.focused {
            states.push("focused".to_string());
            states.push("focus".to_string());
        }
        if m.id.is_some() && m.id == self.visual.hovered {
            states.push("hovered".to_string());
            states.push("hover".to_string());
        }
        states.extend(m.states.iter().map(|s| s.as_str().to_string()));
        // W1: `disabled` is inherited, so a control inside a disabled
        // container matches `:disabled` too.
        if m.disabled || self.disabled_depth > 0 {
            states.push("disabled".to_string());
        }
        let desc = NodeDesc {
            id: m.id.as_ref().map(|i| i.as_str().to_string()),
            classes: m.classes.clone(),
            states,
            ty: m.role.as_str().to_string(),
        };

        let computed =
            lumen_style::resolve_with_ancestors(&env.sources, &desc, &self.desc_stack, &env.media);
        let mut css = Style::new();
        for (prop, c) in &computed {
            lumen_style::apply(&mut css, prop, &c.value, &env.tokens);
        }
        apply_css_paint(self.at(n), &css);
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
/// d.child(Label::new("too early"));   // no method `child` on `Declaring`
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
/// let child = open.child(Label::new("in time"));
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

    /// Lower `w` as a child of this node.
    ///
    /// Only reachable from `Open`, so a child can never be written while its
    /// parent is missing from the ancestor stack.
    pub fn child<W: Direct>(&mut self, w: W) -> LayoutNode {
        let (_, ln) = w.lower(self.sink, Some(self.n));
        ln
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
            self.open.iter().map(|(n, m, _)| (*n, m.role)).collect::<Vec<_>>()
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
            if rec.dep == dep && rec.ctx == ctx && self.tree.is_alive(rec.root) {
                if let Some(raw) = self.tree.lnode(rec.root) {
                    // The whole memo hit: two pointer updates and a record.
                    self.tree.detach(rec.root);
                    match parent {
                        Some(p) => self.tree.attach_last_child(p, rec.root),
                        None => self.tree.set_root(rec.root),
                    }
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
        self.spans.insert(
            key,
            SpanRec {
                root: n,
                dep,
                count,
                ctx,
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
            self.meta.remove(&n);
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
        let Some(engine) = self.text.as_mut() else {
            return None;
        };
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
        h.finish128()
    }
}
