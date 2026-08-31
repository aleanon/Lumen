//! exit — M.6: app-initiated quit through the portable request path
//! (`SystemRequest::Exit`): the shell ends its event loop cleanly, the same
//! path as the window-close button; headless hosts see it as data.
//!
//! E2b: full-form state (`App::with_state`); the root stays an `Element`
//! because `Stack` cannot express centering (align/justify) yet — recorded as
//! a migration gap.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::system::{queue_system, SystemRequest};
use lumen_widgets::{widgets, App, BuildCx, Element, Reactive};

/// App state: whether the quit confirmation is showing.
#[derive(Default, Reactive, serde::Serialize, serde::Deserialize)]
#[reactive(crate = "lumen_core")]
#[serde(default)]
struct ExitState {
    armed: bool,
}

/// Build the exit app.
pub fn main_app() -> App {
    App::with_state(ExitState::default(), |cx: &mut BuildCx, s: &ExitState| {
        let is_armed = *s.armed(cx);
        let content: Vec<Element> = if is_armed {
            vec![
                widgets::text("Really quit?").id("confirm"),
                widgets::button("Yes, exit", |rt| queue_system(rt, SystemRequest::Exit))
                    .id("exit"),
                widgets::button("Cancel", |rt| ExitState::set_armed(rt, false)).id("cancel"),
            ]
        } else {
            vec![
                widgets::text("This app quits itself.").id("blurb"),
                widgets::button("Exit…", |rt| ExitState::set_armed(rt, true)).id("arm"),
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
    })
}
