//! exit — M.6: app-initiated quit through the portable request path
//! (`SystemRequest::Exit`): the shell ends its event loop cleanly, the same
//! path as the window-close button; headless hosts see it as data.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::system::{queue_system, SystemRequest};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the exit app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let armed = cx.signal("armed", || false);
    let is_armed = armed.get(cx.runtime());

    let content: Vec<Element> = if is_armed {
        vec![
            widgets::text("Really quit?").id("confirm"),
            widgets::button("Yes, exit", |rt| queue_system(rt, SystemRequest::Exit)).id("exit"),
            widgets::button("Cancel", move |rt| armed.set(rt, false)).id("cancel"),
        ]
    } else {
        vec![
            widgets::text("This app quits itself.").id("blurb"),
            widgets::button("Exit…", move |rt| armed.set(rt, true)).id("arm"),
        ]
    };
    let mut col = widgets::column(content).id("page");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(12.0),
        ..LayoutStyle::default()
    };
    col
}
