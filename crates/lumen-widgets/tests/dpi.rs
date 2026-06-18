//! HiDPI: changing the scale factor rasterizes at a larger *physical* size
//! while layout (logical px) is unchanged — so hit-testing and geometry stay
//! put and only the pixel buffer grows. scale 1.0 is identical to unscaled.

use lumen_core::geometry::Size;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{App, BuildCx, Element};

fn app() -> App {
    App::new(|_cx: &mut BuildCx| {
        let boxel = Element {
            background: Some(Color::srgb8(0x20, 0x80, 0xf0, 0xff)),
            style: LayoutStyle {
                width: Dim::px(50.0),
                height: Dim::px(50.0),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .id("box");
        Element {
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Dim::pct(1.0),
                height: Dim::pct(1.0),
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                ..LayoutStyle::default()
            },
            children: vec![boxel],
            ..Element::default()
        }
    })
}

/// The `#box` node's bounds `{x,y,w,h}` from the semantics doc.
fn box_bounds(a: &lumen_widgets::Headless) -> (f64, f64, f64, f64) {
    fn walk(n: &serde_json::Value) -> Option<(f64, f64, f64, f64)> {
        if n.get("id").and_then(|v| v.as_str()) == Some("box") {
            let b = n.get("bounds")?;
            return Some((
                b["x"].as_f64()?,
                b["y"].as_f64()?,
                b["w"].as_f64()?,
                b["h"].as_f64()?,
            ));
        }
        n.get("children")?.as_array()?.iter().find_map(walk)
    }
    walk(a.semantics_json().get("root").unwrap()).expect("box node")
}

#[test]
fn raster_scales_but_layout_stays_logical() {
    let mut a = app().run_headless(Size::new(200.0, 200.0));
    a.pump();
    assert_eq!((a.screenshot().width(), a.screenshot().height()), (200, 200));
    let logical = box_bounds(&a); // centred: ~ (75, 75, 50, 50)
    assert!((logical.0 - 75.0).abs() < 1.0 && (logical.1 - 75.0).abs() < 1.0);

    // Go HiDPI: the frame doubles in physical px...
    a.set_scale(2.0);
    assert_eq!((a.screenshot().width(), a.screenshot().height()), (400, 400));
    // ...but the logical layout (hit-test geometry) is unchanged.
    assert_eq!(box_bounds(&a), logical);

    // Back to 1x restores the original frame size.
    a.set_scale(1.0);
    assert_eq!((a.screenshot().width(), a.screenshot().height()), (200, 200));
}
