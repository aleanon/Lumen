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
    overflow(root, &mut out);
    out
}

fn overflow(n: &SemanticsNode, out: &mut Vec<Diagnostic>) {
    for c in &n.children {
        let b = c.bounds;
        let p = n.bounds;
        if b.x1 > p.x1 + 0.5 || b.y1 > p.y1 + 0.5 {
            let who = c.id.as_ref().map(|i| i.as_str()).unwrap_or(&c.label);
            out.push(Diagnostic::new(
                codes::W0103,
                format!(
                    "`{who}` overflows its parent ({:.0}×{:.0} past the edge)",
                    (b.x1 - p.x1).max(0.0),
                    (b.y1 - p.y1).max(0.0)
                ),
            ));
        }
        overflow(c, out);
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
            out.push(Diagnostic::new(
                codes::W0104,
                format!("`{who}` content is clipped ({over:.0} px of ink above/below its box)"),
            ));
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
        out.push(Diagnostic::new(
            codes::W0105,
            format!(
                "`{who}` is interactive but has zero area ({:.0}×{:.0})",
                n.bounds.width(),
                n.bounds.height()
            ),
        ));
    }
    for c in &n.children {
        zero_size(c, out);
    }
}

/// The absolute visual-invariant lint: layout/render correctness checks that
/// should always hold regardless of design — overflow (W0103), clipping (W0104),
/// and zero-area interactive nodes (W0105). Unlike goldens (which catch
/// *changes* vs a baseline), these catch *first-time* defects. Touch-target size
/// and contrast are advisory (design-dependent) and stay separate.
pub fn lint(root: &SemanticsNode) -> Vec<Diagnostic> {
    let mut out = check_overflow(root);
    out.extend(check_clipping(root));
    out.extend(check_zero_size(root));
    out
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
