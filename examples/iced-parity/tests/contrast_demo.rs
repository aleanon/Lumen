//! Manual "try it in action" harness for the design-analysis contrast metric.
//! Run with output visible:
//!
//! ```sh
//! cargo test -p iced-parity --test contrast_demo -- --nocapture
//! ```
//!
//! It drives the real stopwatch app headlessly and prints the APCA contrast
//! report computed from the *actual* display list — same list the renderer
//! consumes — for both the idle and running states.

use lumen_core::geometry::Size;
use lumen_widgets::Headless;

fn print_report(label: &str, h: &mut Headless) {
    let report = h.contrast_report();
    println!("\n===== contrast report: {label} =====");
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

#[test]
fn stopwatch_contrast() {
    // Same size the gallery renders the stopwatch at a comfortable hero canvas.
    let mut a = iced_parity::stopwatch::main_app().run_headless(Size::new(420.0, 360.0));
    a.pump();
    print_report("stopwatch (idle: 'Start')", &mut a);

    // Press Start so the accent button flips to 'Stop' and the clock runs.
    use lumen_agent::dispatch;
    use serde_json::json;
    dispatch(
        &mut a,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "input.click", "params": { "selector": "#toggle" } }),
    );
    a.advance(1500.0); // 1.5s of virtual time → readout shows 00:01
    print_report("stopwatch (running: 'Stop', 00:01)", &mut a);
}
