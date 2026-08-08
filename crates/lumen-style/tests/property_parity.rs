//! SD5.1: every property the parser accepts is either implemented or listed as
//! knowingly unimplemented. There is no third state.
//!
//! The framework's worst defect class for its stated audience is a rule that
//! parses and then does nothing: no error, no effect. A human eventually
//! notices the pixels are wrong; an agent cannot see the screen, so it reports
//! success and moves on. 41 of 78 properties were in that state, and nothing
//! prevented a 42nd.
//!
//! This test is the prevention. Add a property to the parser without either
//! implementing it or declaring it parse-only, and the build fails.

use lumen_style::{APPLIED_PROPERTIES, KNOWN_PROPERTIES, PARSE_ONLY_PROPERTIES};
use std::collections::BTreeSet;

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn known_equals_applied_union_parse_only() {
    let known = set(KNOWN_PROPERTIES);
    let applied = set(APPLIED_PROPERTIES);
    let parse_only = set(PARSE_ONLY_PROPERTIES);

    let accounted: BTreeSet<String> = applied.union(&parse_only).cloned().collect();

    let unaccounted: Vec<_> = known.difference(&accounted).collect();
    assert!(
        unaccounted.is_empty(),
        "these properties parse but are neither implemented nor declared \
         parse-only: {unaccounted:?}\n\
         A property in this state silently does nothing — the exact failure an \
         agent cannot detect. Either implement it in `Style::apply`, or add it \
         to PARSE_ONLY_PROPERTIES so it at least reports W0107."
    );

    let phantom: Vec<_> = accounted.difference(&known).collect();
    assert!(
        phantom.is_empty(),
        "these are claimed applied or parse-only but the parser does not \
         accept them: {phantom:?}\n\
         A property the parser rejects cannot be either — the lists have drifted."
    );
}

#[test]
fn applied_and_parse_only_are_disjoint() {
    // A property cannot be both implemented and knowingly-unimplemented.
    // Overlap would mean an entry outlived its implementation, and W0107 would
    // fire on a property that actually works.
    let overlap: Vec<_> = set(APPLIED_PROPERTIES)
        .intersection(&set(PARSE_ONLY_PROPERTIES))
        .cloned()
        .collect();
    assert!(
        overlap.is_empty(),
        "implemented AND declared parse-only: {overlap:?} — remove them from \
         PARSE_ONLY_PROPERTIES now that they work"
    );
}

#[test]
fn the_gap_is_recorded_and_shrinking() {
    // A tripwire on the headline number, so the backlog cannot quietly grow.
    // Entries leave PARSE_ONLY by being implemented; if this fails upward,
    // something added a property without implementing it.
    assert!(
        PARSE_ONLY_PROPERTIES.len() <= 41,
        "the unimplemented-property backlog grew to {} (was 41 on 2026-08-08)",
        PARSE_ONLY_PROPERTIES.len()
    );
}
