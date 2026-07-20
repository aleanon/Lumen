//! integration — M.6: Lumen as a COMPONENT inside an app that owns its own
//! render loop. The host (see `examples/win.rs`) runs its own winit + wgpu
//! scene and embeds a Lumen `Headless` as a HUD: input is forwarded into the
//! one queue, `pump()` + `screenshot()` produce the HUD frame, and the host
//! uploads it as a texture — Lumen never touches the host's event loop or
//! device.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// The embedded HUD app: a live counter the host forwards clicks into.
pub fn hud_app() -> App {
    App::new(build)
}

/// (Also runnable standalone for the headless smoke.)
pub fn main_app() -> App {
    hud_app()
}

fn build(cx: &mut BuildCx) -> Element {
    let clicks = cx.signal("clicks", || 0i32);
    let v = clicks.get(cx.runtime());
    let mut col = widgets::column(vec![
        widgets::text("Lumen HUD (embedded)").id("title"),
        widgets::text(format!("host clicks: {v}")).id("clicks"),
        widgets::button("+1 from HUD", move |rt| clicks.update(rt, |c| *c += 1)).id("bump"),
    ])
    .id("hud");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(8.0),
        ..LayoutStyle::default()
    };
    col.background = Some(lumen_core::Color::srgb8(0x10, 0x14, 0x1f, 0xff));
    col
}
