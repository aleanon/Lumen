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
    /// Nodes begun but not yet ended, innermost last.
    ///
    /// The first cut kept every record in `meta` from `begin` and reached it
    /// through `meta.get_mut(&n)` on *every* property setter — eight hashed
    /// lookups for a `Button`, where the `Element` path writes struct fields
    /// and inserts once. That alone made direct lowering measurably slower
    /// than the path it was supposed to beat.
    pub(crate) open: Vec<(NodeIndex, Meta)>,
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
        self.open.push((
            n,
            Meta {
                role,
                ..Meta::default()
            },
        ));
        n
    }

    /// The record under construction. Innermost-last, so the common case is
    /// the last slot and no hashing happens at all.
    fn at(&mut self, n: NodeIndex) -> &mut Meta {
        let i = self
            .open
            .iter()
            .rposition(|(k, _)| *k == n)
            .expect("node begun but not ended");
        &mut self.open[i].1
    }

    /// Read the record under construction.
    fn peek(&self, n: NodeIndex) -> &Meta {
        let i = self
            .open
            .iter()
            .rposition(|(k, _)| *k == n)
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
        let style = match self.pending_css.remove(&n) {
            None => style,
            Some(css) => {
                styled = style.clone();
                apply_css_layout(&mut styled, &css);
                self.desc_stack.pop();
                &styled
            }
        };
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
            .rposition(|(k, _)| *k == n)
            .expect("node begun but not ended");
        let (_, meta) = self.open.remove(i);
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
        let n = out.begin(parent, Role::Text);
        let (s, _dyn_text) = text.into_parts();
        out.label(n, s.clone());
        out.text(n, s, style);
        let disabled = apply_common(out, n, common);
        // The cascade runs here: after the widget has declared everything a
        // selector can match on, and before it would lower any children.
        out.resolve(n);
        let mut ls = LayoutStyle::default();
        if let Some(px) = width {
            ls.width = Dim::px(px);
        }
        let ln = out.end(n, &ls, &[], disabled);
        (n, ln)
    }
}

impl Direct for crate::Button {
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode) {
        let (label, on_press, fill, ink, common) = self.into_parts();
        let n = out.begin(parent, Role::Button);
        let (s, _dyn_text) = label.into_parts();
        out.label(n, s.clone());
        out.actions(n, vec![Action::Click, Action::Focus]);
        out.focusable(n, true);
        out.background(n, fill);
        out.corner_radius(n, 8.0);
        if let Some(h) = on_press {
            out.on_click(n, h);
        }
        out.text(
            n,
            s,
            TextStyle {
                font_size: 15.0,
                weight: 600.0,
                color: ink,
                ..TextStyle::default()
            },
        );
        let disabled = apply_common(out, n, common);
        out.resolve(n);
        let ls = LayoutStyle {
            padding: Edges {
                left: Dim::px(16.0),
                right: Dim::px(16.0),
                top: Dim::px(9.0),
                bottom: Dim::px(9.0),
            },
            ..LayoutStyle::default()
        };
        let ln = out.end(n, &ls, &[], disabled);
        (n, ln)
    }
}

impl Direct for crate::ProgressBar {
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode) {
        let (frac, width, height, ink, common) = self.into_parts();
        let n = out.begin(parent, Role::Progress);
        out.value(n, format!("{:.0}%", frac * 100.0));
        out.background(n, Color::srgb8(0xe3, 0xe6, 0xeb, 0xff));
        out.corner_radius(n, 5.0);
        // The `Common` lands BEFORE `resolve`, or a caller's `.id()`/`.class()`
        // is invisible to the cascade — a silent miss, not an error. This
        // ordering is the obligation the direct design moves from the engine
        // onto each widget author; `direct_cascade.rs` pins it.
        let disabled = apply_common(out, n, common);
        out.resolve(n);

        // The fill child, lowered while this bar is on the ancestor stack.
        let f = out.begin(Some(n), Role::Generic);
        out.elide(f, true);
        out.class(f, "fill".to_string());
        out.background(f, ink);
        out.corner_radius(f, 5.0);
        out.resolve(f);
        let fill_ln = out.end(
            f,
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
        let ln = out.end(n, &ls, &[fill_ln], disabled);
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
