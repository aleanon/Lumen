//! WCAG 2.2 automated checks (T7.4): the parts of accessibility conformance a
//! machine can verify — text **contrast ratio** and **accessible names** over
//! the semantic tree. Touch-target size is covered by [`crate::audit`].
//!
//! Screen-reader behaviour (VoiceOver/NVDA/Orca) and the rest of WCAG are the
//! manual/CI checklist in `docs/a11y-checklist.md`.

use lumen_core::semantics::{Action, SemanticsNode};
use lumen_core::Color;

/// Relative luminance (WCAG): Lumen colors are linear-light, so this is the
/// weighted sum directly.
fn luminance(c: Color) -> f64 {
    0.2126 * c.r as f64 + 0.7152 * c.g as f64 + 0.0722 * c.b as f64
}

/// WCAG contrast ratio between two colors (1.0 .. 21.0).
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Whether `fg` on `bg` meets WCAG AA (4.5:1 normal text, 3:1 large text).
pub fn meets_aa(fg: Color, bg: Color, large_text: bool) -> bool {
    let min = if large_text { 3.0 } else { 4.5 };
    contrast_ratio(fg, bg) >= min
}

/// An interactive node missing an accessible name.
#[derive(Clone, Debug, PartialEq)]
pub struct NameIssue {
    /// The node's id, if any.
    pub id: Option<String>,
    /// The node's role (as a string).
    pub role: String,
}

/// Audit the tree for interactive nodes (a `Click` action) with no accessible
/// name (neither label nor value) — a WCAG 4.1.2 failure.
pub fn audit_names(root: &SemanticsNode) -> Vec<NameIssue> {
    let mut out = Vec::new();
    visit(root, &mut out);
    out
}

fn visit(n: &SemanticsNode, out: &mut Vec<NameIssue>) {
    let interactive = n.actions.iter().any(|a| matches!(a, Action::Click));
    let named = !n.label.is_empty() || n.value.as_deref().is_some_and(|v| !v.is_empty());
    if interactive && !named {
        out.push(NameIssue {
            id: n.id.as_ref().map(|i| i.as_str().to_string()),
            role: n.role.as_str().to_string(),
        });
    }
    for c in &n.children {
        visit(c, out);
    }
}
