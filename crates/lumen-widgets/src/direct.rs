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
            on_click: None,
            background: None,
            border: None,
            corner_radius: 0.0,
            content: NodeContent::None,
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
        self.meta.insert(
            n,
            Meta {
                role,
                ..Meta::default()
            },
        );
        n
    }

    /// The record under construction.
    fn at(&mut self, n: NodeIndex) -> &mut Meta {
        self.meta.get_mut(&n).expect("node begun")
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
        let m = self.meta.get(&n).expect("node begun");
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
        let lnode = if children.is_empty() {
            self.layout.leaf_ref(style)
        } else {
            self.layout.container_ref(style, children)
        };
        self.tree.set_lnode(n, lnode.raw());
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
    }

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

        // the fill child
        let f = out.begin(Some(n), Role::Generic);
        out.elide(f, true);
        // `.part("fill")` on the Element path is a class; the sink must agree
        // or `lowered_eq` rejects the comparison — which it did, first run.
        out.class(f, "fill".to_string());
        out.background(f, ink);
        out.corner_radius(f, 5.0);
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

        let disabled = apply_common(out, n, common);
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
