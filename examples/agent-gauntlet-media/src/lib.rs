//! M6-exit gauntlet app: a media-rich, animated showcase — an SVG logo, a
//! procedural video frame (stepped by the agent), a shared-element hero whose
//! bounds morph via the motion system, and a rich-text editor — built once and
//! driven through lumen-agent within the 120fps frame budget.

use lumen_render::media::{TestPattern, VideoSource};
use lumen_render::svg;
use lumen_style::anim::Easing;
use lumen_style::motion::SharedElement;
use lumen_widgets::{widgets, widgets_m4, App, BuildCx, Element};

const LOGO: &str =
    "<svg width=\"48\" height=\"48\"><circle cx=\"24\" cy=\"24\" r=\"20\" fill=\"#1a73e8\"/><rect x=\"18\" y=\"18\" width=\"12\" height=\"12\" fill=\"#ffffff\"/></svg>";

/// Build the media showcase app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let frame = cx.signal("frame", || 0i64);
    let f = frame.get(cx.runtime());

    // SVG logo (rendered to pixels) + the current video frame.
    let logo = widgets::image(svg::render(LOGO, 48, 48, lumen_core::Color::WHITE)).id("logo");
    let video = widgets::image(TestPattern.frame_at(f as f64 * 0.1, 64, 48)).id("video");

    // Shared-element hero: its width morphs with the frame (a transition demo).
    let hero = SharedElement {
        from: kurbo::Rect::new(0.0, 0.0, 40.0, 40.0),
        to: kurbo::Rect::new(0.0, 0.0, 120.0, 40.0),
        duration_ms: 1000.0,
        easing: Easing::EaseInOut,
    };
    let w = hero.bounds_at(f as f64 * 100.0).width();
    let hero_el = Element {
        role: lumen_core::semantics::Role::Image,
        background: Some(lumen_core::Color::srgb8(0x2e, 0xa0, 0x43, 0xff)),
        style: lumen_layout::LayoutStyle {
            width: lumen_layout::Dim::px(w as f32),
            height: lumen_layout::Dim::px(40.0),
            ..Default::default()
        },
        ..Element::default()
    }
    .id("hero");

    let next = widgets::button("Next frame", move |rt| frame.update(rt, |x| *x += 1)).id("next");
    let editor = widgets_m4::rich_text_editor(cx, "notes", "type *notes*");

    widgets::column(vec![
        widgets::row(vec![
            logo,
            widgets::text(format!("frame: {f}")).id("frame-label"),
        ]),
        video,
        hero_el,
        next,
        editor,
    ])
    .id("root")
}
