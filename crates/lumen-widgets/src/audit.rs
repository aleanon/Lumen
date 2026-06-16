//! Touch-target accessibility audit (T3.5). Interactive nodes should be at
//! least 44×44 logical px (Material / Apple HIG guidance). The audit walks the
//! semantic tree — the same tree the agent and tests see — so it runs headless.

use lumen_core::semantics::{Action, SemanticsNode};

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
