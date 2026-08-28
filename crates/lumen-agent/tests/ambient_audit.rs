//! O0.3: lint findings announce themselves instead of waiting to be asked for.
//!
//! `ui.lint` is interrogative — it answers well, but only if the caller already
//! suspects something and names it. A human looking at a window learns the same
//! things ambiently and without a hypothesis. The ambient audit is that
//! equivalent: each painted frame it diffs the findings and pushes the *new*
//! ones into the log ring, which `app.logs {since}` already pages.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element};
use serde_json::json;

fn call(h: &mut lumen_widgets::Headless, m: &str, p: serde_json::Value) -> serde_json::Value {
    lumen_agent::dispatch(
        h,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}

fn w0103_entries(h: &mut lumen_widgets::Headless) -> Vec<String> {
    let logs = call(h, "app.logs", json!({}));
    logs["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["message"].as_str().unwrap_or("").to_string())
        .filter(|m| m.contains("W0103"))
        .collect()
}

/// A held finding is logged once, not once per frame. The ring holds 1000
/// entries; re-logging every frame would flush it in seconds and take every
/// other finding with it.
#[test]
fn a_finding_is_logged_once_not_every_frame() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("overflowing label").id("a")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet("#root { width: 400px; } #a { width: 900px; }");
    for _ in 0..25 {
        h.pump();
    }
    let entries = w0103_entries(&mut h);
    assert_eq!(
        entries.len(),
        1,
        "25 painted frames of the same defect must produce ONE entry: {entries:?}"
    );
    assert!(
        entries[0].contains("[#a]"),
        "the entry must name the node, not just the code: {}",
        entries[0]
    );
}

/// The regression O0.1b exists to make possible: two nodes with the same code
/// are two findings. With `Diagnostic.node` always `None` — its state before
/// O0.1b — both keyed to `(W0103, None)` and the second was silently swallowed.
#[test]
fn two_nodes_with_the_same_code_are_two_entries() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![
            widgets::text("first").id("a"),
            widgets::text("second").id("b"),
        ])
        .id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet("#root { width: 400px; } #a { width: 900px; } #b { width: 900px; }");
    h.pump();
    h.pump();
    let entries = w0103_entries(&mut h);
    assert_eq!(
        entries.len(),
        2,
        "each overflowing node is its own finding: {entries:?}"
    );
    assert!(
        entries.iter().any(|m| m.contains("[#a]")) && entries.iter().any(|m| m.contains("[#b]")),
        "both nodes named: {entries:?}"
    );
}

/// Fix it, then re-break it — driven by *state*, which is how a developer
/// actually works. A monotonic seen-set cleared on `rebuild_fresh()` would
/// never re-report, because `pump` calls `rebuild()` and never `rebuild_fresh`.
#[test]
fn a_reintroduced_finding_is_reported_again() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("label").id("a")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));

    let broken = "#root { width: 400px; } #a { width: 900px; }";
    let fixed = "#root { width: 400px; } #a { width: 100px; }";

    h.set_stylesheet(broken);
    h.pump();
    assert_eq!(w0103_entries(&mut h).len(), 1, "first break reported");

    h.set_stylesheet(fixed);
    h.pump();
    h.pump();
    assert_eq!(w0103_entries(&mut h).len(), 1, "fixing adds nothing");

    h.set_stylesheet(broken);
    h.pump();
    assert_eq!(
        w0103_entries(&mut h).len(),
        2,
        "a REGRESSION must be reported again -- this is the whole point of a \
         presence diff over a monotonic seen-set"
    );
}

/// A healthy app must stay silent, or the channel is worthless.
#[test]
fn a_clean_app_logs_nothing() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("hello").id("a")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    for _ in 0..10 {
        h.pump();
    }
    let logs = call(&mut h, "app.logs", json!({}));
    let entries = logs["result"]["entries"].as_array().unwrap();
    assert!(
        entries.is_empty(),
        "a clean app must not fill the ring: {entries:?}"
    );
}

/// O4.6: a diagnostic-sourced entry keeps its structure. `LogEntry` was
/// `{seq, level, message}`, and `Diagnostic::fmt` never printed the node — so
/// flattening a finding into the ring destroyed the identity that makes it
/// actionable and left the consumer regexing prose.
#[test]
fn logged_findings_keep_their_code_and_node() {
    let mut h = App::new(|_cx: &mut BuildCx| -> Element {
        widgets::column(vec![widgets::text("overflowing").id("a")]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet("#root { width: 400px; } #a { width: 900px; }");
    h.pump();

    let logs = call(&mut h, "app.logs", json!({}));
    let e = logs["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["code"].as_str() == Some("W0103"))
        .unwrap_or_else(|| panic!("a coded entry: {logs}"));

    // The HANDLE, not the author id: path-derived, always present (an unnamed
    // node has no `#id` at all), and directly usable as a selector. The
    // friendly id stays recoverable from the rendered message.
    assert!(
        e["node"].as_str().is_some_and(|n| n.starts_with("nx-")),
        "the node anchor must survive into the ring, unparsed: {e}"
    );
    assert!(
        e["message"].as_str().unwrap().contains("[#a]"),
        "and the author's id is still readable in the message: {e}"
    );
    assert!(
        e["frame"].as_u64().is_some(),
        "entries carry the pump that produced them: {e}"
    );
}

/// Free-text causal entries have no code by design — that is the lint/log split
/// working, not a gap.
#[test]
fn handler_written_entries_carry_no_code() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let _ = cx.signal("n", || 0i64);
        widgets::column(vec![
            widgets::button("go", |rt| rt.log("info", "clicked")).id("go")
        ])
        .id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    call(&mut h, "input.click", json!({ "selector": "#go" }));

    let logs = call(&mut h, "app.logs", json!({}));
    let e = logs["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["message"].as_str() == Some("clicked"))
        .unwrap_or_else(|| panic!("the handler's entry: {logs}"));
    assert!(e.get("code").is_none(), "no code on free text: {e}");
}

/// O0.15: the ambient pass is throttled to a 100 ms cadence, because at 858 µs
/// on a 4000-node page it was 27% of every frame and was being asked 60 times a
/// second for an answer nobody reads that often.
///
/// The property that makes the throttle safe is the settle rule: a finding
/// introduced while the view is animating must be reported the moment the view
/// stops, not up to an interval later. This drives exactly that — the finding
/// appears mid-animation, the clock never advances far enough for the interval
/// to elapse, and it must still be in the log once the tree settles.
#[test]
fn a_finding_introduced_mid_animation_is_reported_when_the_tree_settles() {
    use lumen_core::state::Signal;
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let broken = cx.signal("broken", || false).get(cx.runtime());
        let mut e: Element = widgets::text("label").id("a");
        e.style.width = lumen_layout::Dim::px(if broken { 900.0 } else { 100.0 });
        widgets::column(vec![e]).id("root")
    })
    .run_headless(Size::new(400.0, 200.0));
    h.set_stylesheet("#root { width: 400px; }");

    // Animate: many painting pumps, each advancing the clock by one frame —
    // far less than the throttle interval in total for the first stretch.
    for _ in 0..3 {
        h.advance(16.0);
    }
    assert!(
        w0103_entries(&mut h).is_empty(),
        "nothing wrong yet, so nothing logged"
    );

    let s: Signal<bool> = h.runtime().signal("broken", || false);
    s.update(h.runtime(), |v| *v = true);
    // One painting pump introduces it, then the tree settles.
    h.advance(16.0);
    h.pump();

    assert_eq!(
        w0103_entries(&mut h).len(),
        1,
        "an overflow introduced mid-animation must be reported once the view \
         stops changing, whatever the throttle did in between"
    );
}
