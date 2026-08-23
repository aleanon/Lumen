//! Touch-target accessibility audit (T3.5). Interactive nodes should be at
//! least 44×44 logical px (Material / Apple HIG guidance). The audit walks the
//! semantic tree — the same tree the agent and tests see — so it runs headless.

use lumen_core::semantics::{Action, SemanticsNode};
use lumen_core::{codes, Diagnostic};

/// Layout overflow audit (`W0103`): report any child whose laid-out bounds
/// extend beyond its parent's box (a sign of a too-small fixed size). The
/// structured diagnostics let an agent locate and fix layout bugs.
pub fn check_overflow(root: &SemanticsNode) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    overflow(root, &mut out, false);
    out
}

fn overflow(n: &SemanticsNode, out: &mut Vec<Diagnostic>, in_scroll: bool) {
    // A scroll viewport's content overflows it by design — that is what the
    // offset scrolls through (and the viewport clips). The overflow sits
    // arbitrarily deep (the content wrapper flex-shrinks to the viewport and
    // its rows spill past it), so the whole subtree is exempt.
    let in_scroll = in_scroll || n.scroll.is_some();
    for c in &n.children {
        let b = c.bounds;
        let p = n.bounds;
        // Overlay roots (sheets, popups) paint above the flow and anchor to
        // the window, not their structural parent — escaping it is by design.
        if !in_scroll && !c.overlay && (b.x1 > p.x1 + 0.5 || b.y1 > p.y1 + 0.5) {
            let who = c.id.as_ref().map(|i| i.as_str()).unwrap_or(&c.label);
            out.push(
                Diagnostic::new(
                    codes::W0103,
                    format!(
                        "`{who}` overflows its parent ({:.0}×{:.0} past the edge)",
                        (b.x1 - p.x1).max(0.0),
                        (b.y1 - p.y1).max(0.0)
                    ),
                )
                .with_target(c.node.to_wire(), c.id.as_ref()),
            );
        }
        overflow(c, out, in_scroll);
    }
}

/// Clipping audit (`W0104`): report any node whose rendered *ink* extends past
/// its own layout box — content (usually text) is being cut off. This is the
/// intent-vs-result check that plain box audits (overflow) can't see: the box is
/// internally consistent, but the ink inside it exceeds it (e.g. a too-small
/// line-height clipping descenders).
pub fn check_clipping(root: &SemanticsNode) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    clipping(root, &mut out);
    out
}

fn clipping(n: &SemanticsNode, out: &mut Vec<Diagnostic>) {
    if let Some(ink) = n.ink {
        let b = n.bounds;
        // Vertical overflow only: descenders/ascenders cut by a too-short box is
        // real clipping. Horizontal ink overhang is normal typography (glyph side
        // bearings poke past the advance width without being clipped), so ignore
        // it here to avoid flagging ordinary text.
        let over = (ink.y1 - b.y1).max(b.y0 - ink.y0);
        if over > 0.5 {
            let who = n.id.as_ref().map(|i| i.as_str()).unwrap_or(&n.label);
            out.push(
                Diagnostic::new(
                    codes::W0104,
                    format!("`{who}` content is clipped ({over:.0} px of ink above/below its box)"),
                )
                .with_target(n.node.to_wire(), n.id.as_ref()),
            );
        }
    }
    for c in &n.children {
        clipping(c, out);
    }
}

/// Zero-area interactive audit (`W0105`): an interactive node laid out with no
/// width or height is clickable but invisible/unhittable — usually a missing size
/// or empty content.
pub fn check_zero_size(root: &SemanticsNode) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    zero_size(root, &mut out);
    out
}

fn zero_size(n: &SemanticsNode, out: &mut Vec<Diagnostic>) {
    let interactive = n.actions.iter().any(|a| matches!(a, Action::Click));
    if interactive && (n.bounds.width() < 0.5 || n.bounds.height() < 0.5) {
        let who = n.id.as_ref().map(|i| i.as_str()).unwrap_or(&n.label);
        out.push(
            Diagnostic::new(
                codes::W0105,
                format!(
                    "`{who}` is interactive but has zero area ({:.0}×{:.0})",
                    n.bounds.width(),
                    n.bounds.height()
                ),
            )
            .with_target(n.node.to_wire(), n.id.as_ref()),
        );
    }
    for c in &n.children {
        zero_size(c, out);
    }
}

/// Duplicate-`StableId` audit (`W0001`, 02 §2): ids must be unique within
/// the window — selectors resolve the first match, so a duplicate makes the
/// other nodes unreachable to tests, the agent, and a11y relations.
fn check_duplicate_ids(root: &SemanticsNode) -> Vec<Diagnostic> {
    fn walk(n: &SemanticsNode, seen: &mut std::collections::HashMap<String, u32>) {
        if let Some(id) = &n.id {
            *seen.entry(id.as_str().to_string()).or_default() += 1;
        }
        for c in &n.children {
            walk(c, seen);
        }
    }
    let mut seen = std::collections::HashMap::new();
    walk(root, &mut seen);
    let mut dups: Vec<_> = seen.into_iter().filter(|(_, n)| *n > 1).collect();
    dups.sort(); // deterministic output
    dups.into_iter()
        .map(|(id, count)| {
            Diagnostic::new(
                codes::W0001,
                format!(
                    "duplicate StableId `#{id}` on {count} nodes — selectors resolve \
                     the first match, so the rest are unreachable; make ids unique"
                ),
            )
        })
        .collect()
}

/// Unnamed-focusable audit (`W0301`, 03 §1): every focusable leaf must carry
/// a non-empty label or value — otherwise it is invisible to a11y and
/// unaddressable by name for tests and the agent.
fn check_unnamed_focusable(n: &SemanticsNode, out: &mut Vec<Diagnostic>) {
    let focusable = n.actions.iter().any(|a| matches!(a, Action::Focus));
    if focusable
        && n.children.is_empty()
        && n.label.trim().is_empty()
        && n.value.as_deref().unwrap_or("").trim().is_empty()
    {
        // No author id by definition (an unnamed node is the finding), so the
        // path-derived handle is the ONLY identity this can carry — which is
        // exactly why `handle` had to exist separately from `node`.
        out.push(
            Diagnostic::new(
                codes::W0301,
                format!(
                    "focusable {} leaf ({}) has no label or value — name it \
                     so a11y and selectors can reach it",
                    n.role.as_str(),
                    n.node
                ),
            )
            .with_handle(n.node.to_wire()),
        );
    }
    for c in &n.children {
        check_unnamed_focusable(c, out);
    }
}

/// The absolute visual-invariant lint: layout/render correctness checks that
/// should always hold regardless of design — overflow (W0103), clipping (W0104),
/// zero-area interactive nodes (W0105), duplicate ids (W0001), and unnamed
/// focusables (W0301). Unlike goldens (which catch *changes* vs a baseline),
/// these catch *first-time* defects. Touch-target size and contrast are
/// advisory (design-dependent) and stay separate.
pub fn lint(root: &SemanticsNode) -> Vec<Diagnostic> {
    let mut out = check_overflow(root);
    out.extend(check_clipping(root));
    out.extend(check_zero_size(root));
    out.extend(check_duplicate_ids(root));
    check_unnamed_focusable(root, &mut out);
    check_unvirtualized_list(root, &mut out);
    out
}

/// VL1: how many direct children a scroll container may lay out before the
/// lint suggests virtualizing.
///
/// Chosen well above "a menu" and well below "a data set". A settings page or a
/// nav list has tens of rows and should never be nagged; anything laying out
/// hundreds every frame is paying per-item cost for a viewport-sized result.
const VIRTUALIZE_HINT_THRESHOLD: usize = 100;

/// VL1: a scroll container laying out a large number of children directly.
///
/// `Scrollable` materializes *every* child on *every* frame — its own doc says
/// "for very long lists, virtualize — this lays out all children" — so frame
/// cost grows with the item count instead of the viewport. `VirtualList`
/// renders only the visible window plus overscan and is flat in item count
/// (measured: 1.15 ms for 1M rows).
///
/// Both have existed for a long time; the problem is discoverability. The
/// cheap-looking widget is the obvious one, and the scalable one is the widget
/// you have to already know exists. A lint is how the framework says so at the
/// moment it matters, to an author (or an agent) who cannot see a frame budget.
///
/// Advisory: a long-but-bounded list is a legitimate choice, so this warns
/// rather than errors.
fn check_unvirtualized_list(n: &SemanticsNode, out: &mut Vec<Diagnostic>) {
    if n.role == lumen_core::semantics::Role::ScrollArea {
        // Count the whole subtree, not direct children: a scroll container
        // usually wraps its items in a content node, so `children.len()` is 1
        // no matter how many rows there are. Subtree size is also the honest
        // measure of the cost, since everything under it is built and laid out.
        fn subtree(n: &SemanticsNode) -> usize {
            1 + n.children.iter().map(subtree).sum::<usize>()
        }
        let count = subtree(n) - 1;
        if count > VIRTUALIZE_HINT_THRESHOLD {
            let who =
                n.id.as_ref()
                    .map(|i| format!("`{}`", i.as_str()))
                    .unwrap_or_else(|| "a scroll container".to_string());
            out.push(
                Diagnostic::new(
                    codes::W0108,
                    format!(
                        "{who} lays out {count} nodes; every one is built \
                 and laid out on every frame. `VirtualList` materializes only \
                 the visible window (flat in item count) — consider it past \
                 ~{VIRTUALIZE_HINT_THRESHOLD} items."
                    ),
                )
                .with_target(n.node.to_wire(), n.id.as_ref()),
            );
        }
    }
    for c in &n.children {
        check_unvirtualized_list(c, out);
    }
}

/// An interactive node whose tappable area is below the minimum.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchViolation {
    /// The node's stable id, if any.
    pub id: Option<String>,
    /// The node's label.
    pub label: String,
    /// Measured width (logical px).
    pub width: f64,
    /// Measured height (logical px).
    pub height: f64,
}

/// Walk `root` and return every interactive node smaller than `min` in either
/// dimension. A node is interactive if it exposes a `Click` action.
pub fn audit_touch_targets(root: &SemanticsNode, min: f64) -> Vec<TouchViolation> {
    let mut out = Vec::new();
    visit(root, min, &mut out);
    out
}

fn visit(n: &SemanticsNode, min: f64, out: &mut Vec<TouchViolation>) {
    let interactive = n.actions.iter().any(|a| matches!(a, Action::Click));
    if interactive {
        let (w, h) = (n.bounds.width(), n.bounds.height());
        // Half-px tolerance for sub-pixel layout rounding.
        if w + 0.5 < min || h + 0.5 < min {
            out.push(TouchViolation {
                id: n.id.as_ref().map(|i| i.as_str().to_string()),
                label: n.label.clone(),
                width: w,
                height: h,
            });
        }
    }
    for c in &n.children {
        visit(c, min, out);
    }
}
