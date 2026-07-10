//! B.7 (docs/plan-remediation-2026-07.md): widget parts (04 §5) —
//! `slider .track` / `slider .thumb` / `progress .fill` style the widget's
//! internals. Parts are classes on the internal elements; the ancestor-chain
//! matching from B.1 provides the scoping.

use kurbo::Size;
use lumen_widgets::{col, App, ProgressBar, Slider};

#[test]
fn slider_track_and_thumb_take_part_styles() {
    let sheet = "slider .track { background: #00ff00; } \
                 slider .thumb { background: #ff0000; }";
    let mut h = App::new(|cx| col![Slider::new(cx, "v", 0.0, 100.0).id("s")])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 100.0));
    h.pump();

    let b = h.node_bounds_by_id("s").unwrap();
    let shot = h.screenshot();
    // Value starts at min → the 16px thumb sits at the track's left edge.
    let thumb = shot.pixel(b.x0 as u32 + 8, b.y0 as u32 + 8);
    // The track band (y 8..12) well to the right of the thumb.
    let track = shot.pixel(b.x0 as u32 + 150, b.y0 as u32 + 10);
    assert!(
        thumb[0] > 200 && thumb[1] < 60,
        "thumb painted red via `slider .thumb`: {thumb:?}"
    );
    assert!(
        track[1] > 200 && track[0] < 60,
        "track painted green via `slider .track`: {track:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn progress_fill_part_styles_the_filled_portion() {
    let sheet = "progress .fill { background: #ff00ff; }";
    let mut h = App::new(|_cx| col![ProgressBar::new(0.5).id("p")])
        .stylesheet(sheet)
        .run_headless(Size::new(300.0, 100.0));
    h.pump();

    let b = h.node_bounds_by_id("p").unwrap();
    let shot = h.screenshot();
    // 50% of a 200px track: the fill covers the left half.
    let fill = shot.pixel(b.x0 as u32 + 50, b.center().y as u32);
    let rest = shot.pixel(b.x0 as u32 + 150, b.center().y as u32);
    assert!(
        fill[0] > 200 && fill[2] > 200 && fill[1] < 60,
        "fill painted magenta via `progress .fill`: {fill:?}"
    );
    assert!(
        rest[0] < 250 || rest[2] < 250 || rest[1] > 60,
        "unfilled track keeps its own color: {rest:?}"
    );
}

#[test]
fn parts_do_not_leak_across_widget_types() {
    // A bare `.fill` exists inside progress only; `slider .fill` must not
    // match it, and `progress .thumb` must not match the slider's thumb.
    let sheet = "slider .fill { background: #ff0000; } \
                 progress .thumb { background: #ff0000; }";
    let mut h = App::new(|cx| {
        col![
            Slider::new(cx, "v", 0.0, 100.0).id("s"),
            ProgressBar::new(0.5).id("p")
        ]
    })
    .stylesheet(sheet)
    .run_headless(Size::new(300.0, 150.0));
    h.pump();

    let s = h.node_bounds_by_id("s").unwrap();
    let p = h.node_bounds_by_id("p").unwrap();
    let shot = h.screenshot();
    let thumb = shot.pixel(s.x0 as u32 + 8, s.y0 as u32 + 8);
    let fill = shot.pixel(p.x0 as u32 + 50, p.center().y as u32);
    // Both keep their built-in blue (#1a73e8) — no red anywhere.
    assert!(thumb[2] > thumb[0], "slider thumb stays blue: {thumb:?}");
    assert!(fill[2] > fill[0], "progress fill stays blue: {fill:?}");
}
