//! Renders the PULSE stopwatch to PNGs and prints the APCA contrast report for
//! each frame (the design-analysis tool used as a design check).
//!
//! ```sh
//! cargo run -p chrono-stopwatch        # writes /tmp/chrono-*.png + reports
//! ```

use lumen_agent::dispatch;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::geometry::Point;
use lumen_widgets::Headless;
use serde_json::{json, Value};

fn click(a: &mut Headless, sel: &str) {
    dispatch(
        a,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "input.click", "params": { "selector": sel } }),
    );
}

/// Find a node's bounds centre by stable id (walks the semantic tree).
fn center_of(a: &Headless, id: &str) -> Option<Point> {
    fn walk(n: &Value, id: &str) -> Option<Point> {
        if n.get("id").and_then(|v| v.as_str()) == Some(id) {
            let b = n.get("bounds")?;
            return Some(Point::new(
                b["x"].as_f64()? + b["w"].as_f64()? / 2.0,
                b["y"].as_f64()? + b["h"].as_f64()? / 2.0,
            ));
        }
        n.get("children")?
            .as_array()?
            .iter()
            .find_map(|c| walk(c, id))
    }
    walk(a.semantics_json().get("root")?, id)
}

/// Move the pointer over a node so it renders in its hover state.
fn hover(a: &mut Headless, id: &str) {
    if let Some(p) = center_of(a, id) {
        a.inject(Event::PointerMove(PointerEvent::at(p)));
        a.pump();
    }
}

fn save(a: &mut Headless, name: &str) {
    let path = format!("/tmp/chrono-{name}.png");
    std::fs::write(&path, a.screenshot().to_png()).unwrap();
    let report = a.contrast_report();
    let worst = report
        .targets
        .iter()
        .min_by(|x, y| x.apca_lc.abs().partial_cmp(&y.apca_lc.abs()).unwrap());
    println!("\n# {name}  -> {path}");
    for t in &report.targets {
        println!(
            "  {:<7} Lc {:>6.1}  {:<10} fg {} on {}  [{}]",
            t.label.as_deref().unwrap_or(""),
            t.apca_lc,
            format!("{:?}", t.level),
            t.foreground,
            t.background,
            t.node.as_deref().unwrap_or(""),
        );
    }
    if let Some(w) = worst {
        println!(
            "  weakest: '{}' at Lc {:.1}",
            w.label.as_deref().unwrap_or(""),
            w.apca_lc
        );
    }
}

fn main() {
    // Eclipse (dark), idle.
    let mut a = chrono_stopwatch::run_headless();
    a.pump();
    save(&mut a, "dark-idle");

    // Eclipse (dark), running at 01:27.50.
    click(&mut a, "#toggle");
    a.advance(87_500.0);
    save(&mut a, "dark-running");

    // Daybreak (light), running at 01:27.50.
    let mut b = chrono_stopwatch::run_headless();
    b.pump();
    click(&mut b, "#theme"); // dark -> light
    click(&mut b, "#toggle"); // start
    b.advance(87_500.0);
    save(&mut b, "light-running");

    // Daybreak (light), idle for a clean light reference.
    let mut d = chrono_stopwatch::run_headless();
    d.pump();
    click(&mut d, "#theme");
    save(&mut d, "light-idle");

    // Hover over Start to show the button highlight.
    let mut e = chrono_stopwatch::run_headless();
    e.pump();
    hover(&mut e, "toggle");
    save(&mut e, "dark-hover-start");
}
