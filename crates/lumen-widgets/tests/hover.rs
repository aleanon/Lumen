//! Hover feedback: a clickable node lightens/darkens while the pointer is over
//! it (including over a non-id child, since hover bubbles to the id'd ancestor).

use lumen_core::events::{Event, PointerEvent};
use lumen_core::geometry::{Point, Size};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A centred button (with a child label, like the chrono pills) on a plain bg.
fn app() -> App {
    App::new(|cx: &mut BuildCx| {
        let count = cx.signal("c", || 0i64);
        let mut btn = Element {
            role: lumen_core::semantics::Role::Button,
            label: "Go".into(),
            focusable: true,
            actions: vec![
                lumen_core::semantics::Action::Click,
                lumen_core::semantics::Action::Focus,
            ],
            on_click: Some(std::rc::Rc::new(move |rt| count.update(rt, |c| *c += 1))),
            background: Some(Color::srgb8(0x2d, 0xd4, 0xbf, 0xff)),
            corner_radius: 10.0,
            style: LayoutStyle {
                display: Display::Flex,
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                padding: lumen_layout::Edges::all(Dim::px(20.0)),
                ..LayoutStyle::default()
            },
            children: vec![widgets::text("Go")],
            ..Element::default()
        };
        btn = btn.id("go");
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
            children: vec![btn],
            ..Element::default()
        }
    })
}

#[test]
fn button_tints_on_hover() {
    let mut a = app().run_headless(Size::new(200.0, 200.0));
    a.pump();
    // A corner of the button's fill (centre is under the label glyphs).
    let rest = sample(&mut a, 0.5, 0.62);

    // Move the pointer over the button (its centre is the window centre).
    a.inject(Event::PointerMove(PointerEvent::at(Point::new(
        100.0, 100.0,
    ))));
    a.pump();
    let hovered = sample(&mut a, 0.5, 0.62);

    assert_ne!(rest, hovered, "button fill should change colour on hover");
    assert!(
        a.semantics_json().to_string().contains("hovered"),
        "the id'd button should report the hovered state"
    );

    // Move away: back to the resting colour.
    a.inject(Event::PointerMove(PointerEvent::at(Point::new(5.0, 5.0))));
    a.pump();
    assert_eq!(
        sample(&mut a, 0.5, 0.62),
        rest,
        "leaving restores the colour"
    );
}

/// Sample a pixel at fractional (fx, fy) of the frame.
fn sample(a: &mut lumen_widgets::Headless, fx: f64, fy: f64) -> [u8; 4] {
    let img = a.screenshot();
    let x = (fx * img.width() as f64) as u32;
    let y = (fy * img.height() as f64) as u32;
    let i = ((y * img.width() + x) * 4) as usize;
    let p = img.pixels();
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}
