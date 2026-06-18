//! Regression: resizing the surface must re-layout (so hit-test bounds track
//! the new size) and re-rasterize at the new pixel size (so the presenter
//! doesn't upscale an old-size frame → blur). Mirrors the two desktop-shell
//! resize bugs.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A single centred button in a full-window container, counting its clicks.
fn app() -> App {
    App::new(|cx: &mut BuildCx| {
        let count = cx.signal("count", || 0i64);
        let rt = cx.runtime();
        let btn = widgets::button("hit me", move |rt| count.update(rt, |c| *c += 1)).id("btn");
        Element {
            role: lumen_core::semantics::Role::Group,
            label: format!("count={}", count.get(rt)),
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

fn click_at(a: &mut lumen_widgets::Headless, p: Point) {
    let pe = PointerEvent {
        pos: p,
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
}

#[test]
fn click_area_tracks_resize() {
    let mut a = app().run_headless(Size::new(200.0, 200.0));
    a.pump();

    // The button is centred, so a click at the window centre hits it.
    click_at(&mut a, Point::new(100.0, 100.0));
    assert!(
        a.semantics_json().to_string().contains("count=1"),
        "click at original centre should register"
    );

    // Grow the window. The button recentres to the new middle.
    a.resize(Size::new(600.0, 400.0));
    assert_eq!(
        a.screenshot().width(),
        600,
        "frame must re-rasterize at new width"
    );
    assert_eq!(
        a.screenshot().height(),
        400,
        "frame must re-rasterize at new height"
    );

    // A click at the OLD centre must now miss (the button moved)...
    click_at(&mut a, Point::new(100.0, 100.0));
    assert!(
        a.semantics_json().to_string().contains("count=1"),
        "old centre should no longer hit the button after resize"
    );
    // ...and a click at the NEW centre must hit.
    click_at(&mut a, Point::new(300.0, 200.0));
    assert!(
        a.semantics_json().to_string().contains("count=2"),
        "click at new centre should register after resize"
    );
}
