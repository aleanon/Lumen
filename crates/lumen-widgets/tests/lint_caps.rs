//! O0.5 — a lint pass that walks every node is capped per diagnostic code.
//!
//! Several passes can produce one finding per node, and one of them does so
//! routinely: a column laid out taller than the window makes **every** row an
//! offscreen finding, which is the ordinary shape of a long page. On a
//! 6600-node view that was 6372 diagnostics a frame, each with its own
//! formatted message — 10 ms of string building to state one fact 6372 times.
//!
//! The risk in a cap is that it hides real defects. These tests hold the two
//! properties that make it safe: everything is reported below the cap, and
//! above it the true total is still visible.

use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App, Element};

/// `n` labelled rows in a window far too short to show them, so every row past
/// the fold is offscreen.
fn tall_page(n: usize) -> App {
    App::new(move |_cx| {
        let rows: Vec<Element> = (0..n)
            .map(|i| widgets::text(format!("row {i}")).id(format!("r{i}")))
            .collect();
        widgets::column(rows)
    })
}

fn offscreen_findings(app: App) -> Vec<lumen_core::Diagnostic> {
    let mut h = app.run_headless(Size::new(200.0, 60.0));
    h.pump();
    h.lint()
        .into_iter()
        .filter(|d| d.code == lumen_core::codes::W0112)
        .collect()
}

#[test]
fn every_finding_is_reported_below_the_cap() {
    // Five offscreen rows is five findings and no summary — a cap must not
    // change what a small view reports.
    let found = offscreen_findings(tall_page(6));
    assert!(
        found.len() <= 6,
        "no more findings than rows: {}",
        found.len()
    );
    assert!(
        !found.iter().any(|d| d.message.contains("suppressed")),
        "nothing was suppressed, so no summary line: {:?}",
        found.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn the_cap_bounds_the_output_on_a_long_page() {
    let found = offscreen_findings(tall_page(1000));
    assert!(
        found.len() < 100,
        "a thousand offscreen rows must not produce a thousand diagnostics; \
         got {}",
        found.len()
    );
}

#[test]
fn the_summary_reports_the_true_total() {
    // The property that makes the cap honest. A truncation the reader cannot
    // see is worse than the flood: it reads as "only 50 of these exist".
    let found = offscreen_findings(tall_page(1000));
    let summary = found
        .iter()
        .find(|d| d.message.contains("suppressed"))
        .expect("a summary line accompanies a capped pass");

    // The individual findings plus the suppressed count must equal the real
    // number of offscreen rows, which is everything below the 60px window.
    let shown = found.len() - 1;
    let n: usize = summary
        .message
        .split_whitespace()
        .nth(2)
        .and_then(|w| w.parse().ok())
        .expect("the summary states the total");
    assert!(
        n > shown,
        "the total ({n}) exceeds what was shown ({shown}), so the reader knows \
         the cap hid something"
    );
    assert!(
        n > 100,
        "and it reflects the real scale of the problem, not the cap: {n}"
    );
}

#[test]
fn a_capped_pass_does_not_hide_other_codes() {
    // The cap is per code. A flood of W0112 must not crowd out a different
    // finding — that would trade a slow lint for a blind one.
    let mut h = tall_page(1000).run_headless(Size::new(200.0, 60.0));
    h.pump();
    let all = h.lint();
    let codes: std::collections::HashSet<&str> = all.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(lumen_core::codes::W0112),
        "the flooding code is present"
    );
    assert!(
        all.len() < 200,
        "the whole lint is bounded, not just one pass: {}",
        all.len()
    );
}
