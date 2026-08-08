//! SD5.0: the diagnostic registry (`diagnostics.md`) is stable API (ADR-019),
//! so it must not drift from the `codes` module that implements it.
//!
//! It drifted once already — 9 documented rows against 16 defined consts — and
//! the gap caused a real collision: a proposed `W0105` for parse-only `.lss`
//! properties clashed with the live zero-area-interactive-node code. This test
//! turns "remember to append a row" from a discipline into a build failure.

use std::collections::BTreeSet;

/// Every code the registry documents, parsed out of the markdown table.
fn documented_codes() -> BTreeSet<String> {
    let md = include_str!("../diagnostics.md");
    md.lines()
        .filter_map(|line| {
            // Table rows look like `| W0001 | warning | … |`. The "next free"
            // bullets below the table are prose, not rows, so they're skipped
            // by the leading-pipe requirement.
            let rest = line.strip_prefix('|')?;
            let cell = rest.split('|').next()?.trim();
            is_code(cell).then(|| cell.to_string())
        })
        .collect()
}

/// Every code the `codes` module defines, parsed out of its source.
///
/// Parsing the source rather than reflecting over the module is deliberate:
/// Rust has no way to enumerate a module's consts, and a hand-maintained list
/// here would be a third thing to keep in sync — exactly the failure this test
/// exists to prevent.
fn defined_codes() -> BTreeSet<String> {
    let src = include_str!("../src/diagnostics.rs");
    src.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub const ")?;
            let name = rest.split(':').next()?.trim();
            is_code(name).then(|| name.to_string())
        })
        .collect()
}

/// `E####` or `W####`.
fn is_code(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('E' | 'W')) && s.len() == 5 && chars.all(|c| c.is_ascii_digit())
}

#[test]
fn registry_documents_every_defined_code() {
    let documented = documented_codes();
    let defined = defined_codes();

    let undocumented: Vec<_> = defined.difference(&documented).collect();
    assert!(
        undocumented.is_empty(),
        "these codes exist in `lumen_core::codes` but have no row in \
         crates/lumen-core/diagnostics.md: {undocumented:?}\n\
         Diagnostic codes are stable API (ADR-019) — agents pattern-match on \
         them. Append a row in the same commit that adds the const."
    );

    let phantom: Vec<_> = documented.difference(&defined).collect();
    assert!(
        phantom.is_empty(),
        "these codes are documented in crates/lumen-core/diagnostics.md but \
         have no `pub const` in `lumen_core::codes`: {phantom:?}\n\
         Either add the const or remove the row — a documented code with no \
         implementation is a promise the framework never keeps."
    );
}

#[test]
fn parser_recognizes_the_code_shape() {
    // Guards the test itself: if `is_code` silently stopped matching, both
    // sets would go empty and the comparison above would pass vacuously.
    assert!(is_code("W0001"));
    assert!(is_code("E0701"));
    assert!(!is_code("W001"), "too short");
    assert!(!is_code("W00011"), "too long");
    assert!(!is_code("X0001"), "wrong severity letter");
    assert!(!is_code("Wabcd"), "non-numeric");
    assert!(!is_code("Code"), "prose");

    assert!(
        !documented_codes().is_empty(),
        "parsed zero codes out of diagnostics.md — the table shape changed"
    );
    assert!(
        !defined_codes().is_empty(),
        "parsed zero codes out of diagnostics.rs — the const shape changed"
    );
}
