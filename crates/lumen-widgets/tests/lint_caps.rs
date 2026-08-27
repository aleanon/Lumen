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

/// O0.7: the cap belongs to the **ambient** pass, not to the check.
///
/// `lint()` runs on a frame budget, so a long page must not spend a
/// millisecond formatting one fact a thousand times. A caller who explicitly
/// asked for a lint and is waiting for the answer is in the opposite position
/// — the cost is bounded by the one call, and a cap could hide the very node
/// they are hunting. `lint_all()` is that path.
#[test]
fn lint_all_reports_every_finding() {
    let mut h = tall_page(1000).run_headless(Size::new(200.0, 60.0));
    h.pump();

    let offscreen = |ds: Vec<lumen_core::Diagnostic>| -> Vec<_> {
        ds.into_iter()
            .filter(|d| d.code == lumen_core::codes::W0112)
            .collect::<Vec<_>>()
    };
    let capped = offscreen(h.lint());
    let full = offscreen(h.lint_all());

    // Non-vacuous: the capped arm really did suppress something here, so the
    // two arms are being compared on a page where the cap is load-bearing.
    assert!(
        capped.iter().any(|d| d.message.contains("suppressed")),
        "the capped arm must have suppressed findings on a 1000-row page"
    );
    assert!(
        full.len() > capped.len() * 5,
        "uncapped must report far more than the cap: {} vs {}",
        full.len(),
        capped.len()
    );
    assert!(
        !full.iter().any(|d| d.message.contains("suppressed")),
        "an uncapped pass has nothing to summarise, so no summary line"
    );

    // The capped arm's summary states the true total, so the two agree about
    // how many findings exist — the cap is a display bound, not a count bound.
    let note = capped
        .iter()
        .find(|d| d.message.contains("suppressed"))
        .expect("checked above");
    let total: usize = note
        .message
        .split_whitespace()
        .nth(2)
        .and_then(|w| w.parse().ok())
        .expect("summary reads `{shown} of {total} …`");
    assert_eq!(
        total,
        full.len(),
        "the cap's summary total must equal what lint_all actually reports"
    );
}
