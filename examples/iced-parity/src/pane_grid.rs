//! pane_grid — two resizable panes; drag the split to move it (E8.4). Each pane
//! is a labelled, tinted surface so the divider reads clearly; the panes flex to
//! the split ratio (`min_width: 0`, so dragging actually moves them).
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::widgets_extra::pane_grid;
use lumen_widgets::{theme, App, BuildCx, Element};

/// Build the pane-grid app.
pub fn main_app() -> App {
    App::new(build)
}

fn pane(label: &str, bg: Color) -> Element {
    Element {
        role: Role::Group,
        background: Some(bg),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            min_width: Dim::px(0.0), // let the pane flex to the split ratio
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![theme::caption(label)],
        ..Element::default()
    }
}

fn build(cx: &mut BuildCx) -> Element {
    let left = pane("LEFT", theme::surface()).id("left");
    let right = pane("RIGHT", Color::srgb8(0xec, 0xee, 0xf2, 0xff)).id("right");
    pane_grid(cx, "split", left, right)
}
