//! [`PickList`] — a dropdown single-select. Its `Element` (a trigger plus, when
//! open, an overlay list) is built inside [`PickList::new`]; the selection and
//! open state live in signals keyed by `name`.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

const W: f64 = 220.0;
const TRIGGER_H: f64 = 38.0;
const ROW_H: f64 = 34.0;

/// A dropdown: the trigger shows the selection (or `placeholder`); clicking it
/// reveals the options, and choosing one stores it under `name`.
/// # Example
///
/// ```
/// use lumen_widgets::{App, PickList};
///
/// let app = App::new(|cx| PickList::new(cx, "pick", "Select…", ["One", "Two", "Three"]).into());
/// # lumen_widgets::doc_shot(app, 200.0, 60.0, "pick_list");
/// ```
///
/// Renders:
///
/// ![Pick List example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/pick_list.png)
///
/// The picture above is `src/doc_shots/pick_list.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct PickList {
    el: Element,
}

fn chevron() -> Element {
    widgets::canvas(12.0, 12.0, |f, size| {
        use kurbo::{BezPath, Point};
        let (w, h) = (size.width, size.height);
        let mut p = BezPath::new();
        p.move_to(Point::new(w * 0.2, h * 0.4));
        p.line_to(Point::new(w * 0.5, h * 0.7));
        p.line_to(Point::new(w * 0.8, h * 0.4));
        f.stroke(&p, Color::srgb8(0x6b, 0x72, 0x80, 0xff), 1.6);
    })
}

fn text(s: impl Into<String>, color: Color) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 14.0;
        ts.color = color;
    }
    e
}

impl PickList {
    /// A dropdown over `options`, selection stored under `name`, showing
    /// `placeholder` when nothing is selected.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        placeholder: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> PickList {
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
        let selected = cx.signal(name, String::new);
        let open = cx.signal(&format!("{name}.open"), || false);
        let sel = selected.get(cx.runtime());
        let is_open = open.get(cx.runtime());
        let placeholder = placeholder.into();

        // Trigger: current selection (or placeholder) + a chevron.
        let label = if sel.is_empty() {
            text(placeholder, Color::srgb8(0x9a, 0xa1, 0xad, 0xff))
        } else {
            text(sel.clone(), Color::srgb8(0x1c, 0x22, 0x30, 0xff))
        };
        let mut label = label;
        label.style.flex_grow = 1.0;
        let mut trigger = widgets::row(vec![label, chevron()]);
        trigger.role = Role::Button;
        trigger.focusable = true;
        trigger.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
        trigger.corner_radius = 8.0;
        trigger.style.align_items = Some(Align::Center);
        trigger.style.column_gap = Dim::px(8.0);
        trigger.style.height = Dim::px(TRIGGER_H as f32);
        trigger.style.padding = Edges {
            left: Dim::px(12.0),
            right: Dim::px(10.0),
            top: Dim::px(0.0),
            bottom: Dim::px(0.0),
        };
        trigger.on_click = Some(Rc::new(move |rt| open.update(rt, |o| *o = !*o)));

        let mut children = vec![trigger];

        if is_open {
            let rows: Vec<Element> = options
                .iter()
                .map(|opt| {
                    let opt_s = opt.clone();
                    let mut r = widgets::row(vec![text(
                        opt.clone(),
                        Color::srgb8(0x1c, 0x22, 0x30, 0xff),
                    )]);
                    r.style.align_items = Some(Align::Center);
                    r.style.height = Dim::px(ROW_H as f32);
                    r.style.padding = Edges {
                        left: Dim::px(12.0),
                        right: Dim::px(12.0),
                        top: Dim::px(0.0),
                        bottom: Dim::px(0.0),
                    };
                    r.background = if *opt == sel {
                        Some(Color::srgb8(0xed, 0xf2, 0xff, 0xff))
                    } else {
                        Some(Color::srgb8(0xff, 0xff, 0xff, 0xff))
                    };
                    r.on_click = Some(Rc::new(move |rt| {
                        selected.set(rt, opt_s.clone());
                        open.set(rt, false);
                    }));
                    r
                })
                .collect();
            let mut menu = widgets::column(rows);
            menu.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
            menu.corner_radius = 8.0;
            menu.shadow = Some(crate::element::Shadow::soft());
            // Paint above sibling content below the trigger, and escape clips.
            menu.overlay = true;
            // Click-away / Escape closes the dropdown (light dismiss).
            menu.on_dismiss = Some(Rc::new(move |rt| open.set(rt, false)));
            menu.style.position = Position::Absolute;
            menu.style.inset = Edges {
                top: Dim::px((TRIGGER_H + 4.0) as f32),
                left: Dim::px(0.0),
                ..Edges::AUTO
            };
            menu.style.width = Dim::px(W as f32);
            children.push(menu);
        }

        let el = Element {
            role: Role::Group,
            style: LayoutStyle {
                position: Position::Relative,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Dim::px(W as f32),
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        PickList { el }
    }
}

impl_common!(PickList);
