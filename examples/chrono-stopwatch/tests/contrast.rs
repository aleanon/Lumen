//! The design-analysis contrast tool used as a regression gate: every text
//! label in PULSE must stay legible (APCA |Lc| >= 60) in both themes and the
//! running state. This is the "critique-as-data" loop wired into CI — a future
//! palette edit that tanks contrast fails here.

use chrono_stopwatch::run_headless;
use lumen_agent::dispatch;
use lumen_widgets::Headless;
use serde_json::json;

fn click(a: &mut Headless, sel: &str) {
    dispatch(
        a,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "input.click", "params": { "selector": sel } }),
    );
}

const FLOOR: f64 = 60.0;

#[test]
fn all_labels_legible_in_both_themes() {
    for to_light in [false, true] {
        let theme = if to_light { "light" } else { "dark" };
        let mut a = run_headless();
        a.pump();
        if to_light {
            click(&mut a, "#theme");
        }
        click(&mut a, "#toggle"); // start running -> Stop label + active arc
        a.advance(87_500.0); // 01:27.50

        let report = a.contrast_report();
        assert!(!report.targets.is_empty(), "{theme}: expected text targets");
        for t in &report.targets {
            assert!(
                t.apca_lc.abs() >= FLOOR,
                "{theme}: '{}' below contrast floor: Lc {} (fg {} on {})",
                t.label.as_deref().unwrap_or(""),
                t.apca_lc,
                t.foreground,
                t.background,
            );
        }
    }
}

#[test]
fn controls_drive_state() {
    let mut a = run_headless();
    a.pump();
    click(&mut a, "#toggle");
    a.advance(87_500.0);
    let tree = a.semantics_json().to_string();
    assert!(
        tree.contains("01:27"),
        "readout should advance while running"
    );
    assert!(
        tree.contains("Stop"),
        "toggle should read 'Stop' while running"
    );

    click(&mut a, "#reset");
    a.pump();
    let tree = a.semantics_json().to_string();
    assert!(tree.contains("00:00"), "reset should zero the readout");
    assert!(tree.contains("Start"), "reset should stop the timer");
}
