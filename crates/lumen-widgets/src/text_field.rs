//! [`TextField`] — a self-stateful **multi-line** text input. Its `Element` is
//! built inside [`TextField::new`]; the value lives in a signal keyed by `name`.
//! (Single-line input is [`TextInput`](crate::TextInput).)

use crate::element::NodeContent;
use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// A multi-line text area. Committed text is appended to the value signal
/// (`name`); the box is sized for several lines and wraps to its width.
pub struct TextField {
    el: Element,
}

impl TextField {
    /// A multi-line field with `initial` contents, state stored under `name`.
    /// Defaults to ~5 visible lines; override with [`lines`](TextField::lines)
    /// or [`width`](TextField::width).
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> TextField {
        let value = cx.signal(name, || initial.to_string());
        let v = value.get(cx.runtime());
        let shown = if v.is_empty() {
            " ".to_string()
        } else {
            v.clone()
        };
        let line_h = 20.0_f32;
        let el = Element {
            role: Role::TextInput,
            focusable: true,
            label: v.clone(),
            value: Some(v),
            actions: vec![Action::Focus, Action::SetValue],
            background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
            corner_radius: 6.0,
            style: LayoutStyle {
                padding: Edges::all(Dim::px(8.0)),
                min_width: Dim::px(220.0),
                // Several lines tall, and a fixed width so the text wraps
                // (multi-line). Height grows past this via min_height semantics.
                min_height: Dim::px(line_h * 5.0 + 16.0),
                width: Dim::px(260.0),
                ..LayoutStyle::default()
            },
            content: NodeContent::Text(shown, TextStyle::default()),
            on_text: Some(Rc::new(move |rt, t| {
                let t = t.to_string();
                value.update(rt, |s| s.push_str(&t))
            })),
            ..Element::default()
        };
        TextField { el }
    }

    /// Set the visible line count (≈ rows of `20px`).
    pub fn lines(mut self, n: u32) -> TextField {
        self.el.style.min_height = Dim::px(20.0 * n as f32 + 16.0);
        self
    }

    /// Set the wrap width in px.
    pub fn width(mut self, px: f32) -> TextField {
        self.el.style.width = Dim::px(px);
        self
    }
}

impl_common!(TextField);
