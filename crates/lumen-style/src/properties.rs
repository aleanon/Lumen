//! The v1 `.lss` property name set (04 §3). Used for E0102 did-you-mean.

/// All recognized property names.
pub const KNOWN_PROPERTIES: &[&str] = &[
    // layout
    "display",
    "flex-direction",
    "flex-wrap",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "justify-content",
    "align-items",
    "align-self",
    "align-content",
    "gap",
    "row-gap",
    "column-gap",
    "grid-template-columns",
    "grid-template-rows",
    "grid-column",
    "grid-row",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "aspect-ratio",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "position",
    "inset",
    "inset-top",
    "inset-right",
    "inset-bottom",
    "inset-left",
    "overflow",
    // visual
    "background",
    "border",
    // Applied by `apply()` since the borders work — were missing here, so
    // using them raised a spurious E0102 (B.7a).
    "border-width",
    "border-color",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-radius",
    "shadow",
    "opacity",
    "blend-mode",
    "filter",
    "backdrop-filter",
    "clip",
    "transform",
    "transform-origin",
    "z-index",
    "visibility",
    "cursor",
    // typography
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "font-features",
    "font-variation",
    "line-height",
    "letter-spacing",
    "color",
    "text-align",
    "text-overflow",
    "text-wrap",
    "text-decoration",
    "selection-color",
    // motion
    "transition",
    "animation",
    "animation-force",
];

/// CSS property names that Lumen spells differently, mapped to the Lumen name.
///
/// # Why an explicit table rather than a better distance metric
///
/// `did_you_mean` matches on Levenshtein distance ≤ 2, which works well for a
/// typo and **cannot work at all** for a systematic rename: `left` →
/// `inset-left` is distance 6, `background-color` → `background` is 6,
/// `box-shadow` → `shadow` is 4. Raising the threshold would not fix it either
/// — at distance 6 nearly every property matches nearly every other, so the
/// suggestions become noise. These are not near-misses; they are different
/// names for the same thing, and only a table knows that.
///
/// Every entry here is a name a developer types from CSS muscle memory and
/// Lumen legitimately rejects. The rejection is already loud — `E0102` with a
/// file:line:col span, and the stylesheet is dropped atomically — so this only
/// closes the gap between "that is wrong" and "here is what to write".
///
/// The Lumen spellings are deliberate, not accidental. `inset(-…)` regularizes
/// the four-sided box pattern that CSS is itself inconsistent about:
/// `padding`/`padding-left`, `margin`/`margin-left`, `inset`/`inset-left` —
/// where CSS pairs the `inset` shorthand with bare `left`. Nothing here should
/// be read as a plan to accept the CSS spelling: aliasing them for real would
/// double the surface that `KNOWN_PROPERTIES`, `APPLIED_PROPERTIES`, `W0107`
/// and `W0109` each have to track, to save one lookup.
pub const CSS_ALIASES: &[(&str, &str)] = &[
    // Physical inset longhands. CSS: `left`; Lumen: `inset-left`.
    ("left", "inset-left"),
    ("top", "inset-top"),
    ("right", "inset-right"),
    ("bottom", "inset-bottom"),
    // CSS: `box-shadow`; Lumen: `shadow`.
    ("box-shadow", "shadow"),
    // CSS: `mix-blend-mode`; Lumen: `blend-mode`.
    ("mix-blend-mode", "blend-mode"),
    // CSS splits background into longhands; Lumen's `background` takes a color
    // OR a gradient, so `background-color` has no separate spelling.
    ("background-color", "background"),
];

/// The Lumen property a CSS name corresponds to, if it is one Lumen spells
/// differently. See [`CSS_ALIASES`].
pub fn css_alias(name: &str) -> Option<&'static str> {
    CSS_ALIASES
        .iter()
        .find(|(css, _)| *css == name)
        .map(|(_, lumen)| *lumen)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every alias must point at a real property, or the hint sends the author
    /// somewhere that will also be rejected.
    #[test]
    fn every_alias_targets_a_known_property() {
        for (css, lumen) in CSS_ALIASES {
            assert!(
                KNOWN_PROPERTIES.contains(lumen),
                "`{css}` suggests `{lumen}`, which is not a known property"
            );
        }
    }

    /// An alias must not shadow a real property name: if Lumen ever adopts one
    /// of these spellings, the entry has to go rather than silently redirect a
    /// valid declaration.
    #[test]
    fn no_alias_shadows_a_real_property() {
        for (css, _) in CSS_ALIASES {
            assert!(
                !KNOWN_PROPERTIES.contains(css),
                "`{css}` is a real property now — remove it from CSS_ALIASES"
            );
        }
    }
}
