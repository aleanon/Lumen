//! `widget-rating` — a **third-party** star-rating widget built only on the
//! public `lumen-widgets` ABI (`Element`/`BuildCx`), demonstrating that
//! out-of-tree widgets are first-class (T7.2). Distributable via `lumen add`.

use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use lumen_widgets::element::{BuildCx, Element};
use std::rc::Rc;

/// A 0..=`max` star rating. `name` keys the value signal; clicking star *i*
/// sets the rating to *i+1*. Semantics: role `Slider`, value = the rating.
pub fn rating(cx: &BuildCx, name: &str, max: usize) -> Element {
    let value = cx.signal(name, || 0usize);
    let v = value.get(cx.runtime());
    let stars: Vec<Element> = (0..max)
        .map(|i| {
            let filled = i < v;
            Element {
                role: Role::Button,
                label: format!("star {}", i + 1),
                focusable: true,
                actions: vec![Action::Click, Action::Focus],
                states: if filled {
                    vec![SemState::Selected]
                } else {
                    vec![]
                },
                style: LayoutStyle {
                    padding: Edges::all(Dim::px(2.0)),
                    ..LayoutStyle::default()
                },
                text: Some((
                    if filled { "★" } else { "☆" }.to_string(),
                    TextStyle {
                        font_size: 18.0,
                        weight: 400.0,
                        color: Color::srgb8(0xf5, 0xa6, 0x23, 0xff),
                        line_height: None,
                        letter_spacing: 0.0,
                    },
                )),
                on_click: Some(Rc::new(move |rt| value.set(rt, i + 1))),
                ..Element::default()
            }
            .id(format!("{name}-star-{i}"))
        })
        .collect();
    Element {
        role: Role::Slider,
        label: format!("rating {v} of {max}"),
        value: Some(format!("{v}")),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children: stars,
        ..Element::default()
    }
    .id(name)
}
