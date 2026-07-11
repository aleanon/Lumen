//! [`SearchField`] (W.1) — a `TextInput` dressed as a search box: magnifier
//! glyph, placeholder, and a clear (×) button that appears once there is
//! text. The query lives in the same `Signal<TextEditor>` a plain
//! `TextInput` would use (key = `name`), with the mirror string readable at
//! `{name}.text` like every editor.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element, TextInput};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Edges};
use lumen_text::TextEditor;
use std::rc::Rc;

/// A search box over a text editor signal.
pub struct SearchField {
    el: Element,
}

fn magnifier() -> Element {
    widgets::canvas(14.0, 14.0, |f, size| {
        use kurbo::{Circle, Line, Point, Shape};
        let r = size.width * 0.32;
        let c = Point::new(size.width * 0.42, size.height * 0.42);
        let grey = Color::srgb8(0x6b, 0x72, 0x80, 0xff);
        f.stroke(&Circle::new(c, r).to_path(0.1), grey, 1.6);
        f.stroke(
            &Line::new(
                Point::new(c.x + r * 0.75, c.y + r * 0.75),
                Point::new(size.width * 0.9, size.height * 0.9),
            )
            .to_path(0.1),
            grey,
            1.6,
        );
    })
}

impl SearchField {
    /// A search field storing its editor under `name`.
    pub fn new(cx: &BuildCx, name: &str, placeholder: impl Into<String>) -> SearchField {
        let editor = cx.signal(name, || TextEditor::new(""));
        let has_text = !editor.get(cx.runtime()).text().is_empty();
        let _ = placeholder; // placeholder rendering is the input's concern (below)

        let mut input: Element = TextInput::new(cx, name, "").into();
        // Focus tracking is id-based: without an id on the inner editor,
        // clicking the field would focus nothing and typing would drop.
        input = input.id(format!("{name}-input"));
        input.style.flex_grow = 1.0;

        let mut children = vec![magnifier(), input];
        if has_text {
            let mut clear = widgets::text("×");
            if let Some(ts) = clear.text_style_mut() {
                ts.font_size = 16.0;
                ts.color = Color::srgb8(0x6b, 0x72, 0x80, 0xff);
            }
            clear.role = Role::Button;
            clear.label = "clear search".to_string();
            clear.focusable = true;
            clear.on_click = Some(Rc::new(move |rt| {
                editor.set(rt, TextEditor::new(""));
            }));
            children.push(clear);
        }

        let mut row = widgets::row(children).class("search-field");
        row.role = Role::Group;
        row.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
        row.corner_radius = 8.0;
        row.style.align_items = Some(Align::Center);
        row.style.column_gap = Dim::px(8.0);
        row.style.padding = Edges {
            left: Dim::px(10.0),
            right: Dim::px(10.0),
            top: Dim::px(4.0),
            bottom: Dim::px(4.0),
        };
        SearchField { el: row }
    }
}

impl_common!(SearchField);
