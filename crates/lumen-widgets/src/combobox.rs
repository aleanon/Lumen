//! [`Combobox`] (W.2) — Popover + filtering: a text input whose typed query
//! filters an option list; picking a row stores it in `{name}.selected` and
//! mirrors it into the input. State: the editor under `{name}` (standard
//! TextInput contract), open flag `{name}.open`, selection `{name}.selected`.

use crate::widget::{impl_widget, Common, Widget};
use crate::{widgets, BuildCx, Element, TextInput};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_text::TextEditor;
use std::rc::Rc;

const W: f64 = 240.0;
const ROW_H: f64 = 32.0;
/// How many matches the panel shows before it starts scrolling. Past this many
/// the list becomes a windowed [`VirtualList`](crate::VirtualList), so filtering
/// a long option set stays O(visible) and the tail is still reachable.
const MAX_VISIBLE: usize = 8;

/// A filtering dropdown over a fixed option list.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{top, Combobox, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     top(cx, Combobox::new(cx, "fruit", ["Apple", "Banana", "Cherry"]).into())
/// }
/// # let app = App::new(build);
/// # // Rendered with the option list open (`fruit.open`).
/// # lumen_widgets::doc_shot_open(app, 220.0, 200.0, "combobox", "fruit.open");
/// ```
///
/// Renders:
///
/// ![Combobox example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/combobox.png)
///
/// The picture above is `src/doc_shots/combobox.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Combobox {
    /// The field, still unbuilt — `TextInput` is a `Widget`.
    input: TextInput,
    name: String,
    open: lumen_core::state::Signal<bool>,
    /// The dropdown panel, or `None` while closed.
    ///
    /// **This one is built eagerly**, and it marks the limit of deferral: the
    /// panel may contain a `VirtualList`, whose construction needs the
    /// `BuildCx` (it owns a scroll signal). A widget can only defer as far as
    /// the last thing that needs the build context — everything past that has
    /// to be materialized where the context is.
    menu: Option<Element>,
    common: Common,
}

impl Combobox {
    /// A text field with a filtered dropdown of `options`.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Combobox {
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
        let editor = cx.signal(name, || TextEditor::new(""));
        let open = cx.signal(format!("{name}.open"), || false);
        let selected = cx.signal(format!("{name}.selected"), String::new);
        let query = editor.get(cx.runtime()).text().to_string();
        let is_open = open.get(cx.runtime());

        let menu = is_open.then(|| {
            let q = query.to_lowercase();
            let rows: Vec<Element> = options
                .iter()
                .filter(|o| q.is_empty() || o.to_lowercase().contains(&q))
                .map(|opt| {
                    let opt_s = opt.clone();
                    let mut t = widgets::text(opt.clone());
                    if let Some(ts) = t.text_style_mut() {
                        ts.font_size = 14.0;
                    }
                    let mut r = widgets::row(vec![t]);
                    r.role = Role::Button;
                    r.focusable = true;
                    r.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
                    r.style.align_items = Some(Align::Center);
                    r.style.height = Dim::px(ROW_H as f32);
                    r.style.padding = Edges {
                        left: Dim::px(10.0),
                        right: Dim::px(10.0),
                        top: Dim::px(0.0),
                        bottom: Dim::px(0.0),
                    };
                    r.on_click = Some(Rc::new(move |rt| {
                        selected.set(rt, opt_s.clone());
                        editor.set(rt, TextEditor::new(&opt_s));
                        open.set(rt, false);
                    }));
                    r
                })
                .collect();
            let empty = rows.is_empty();
            let mut menu = if rows.len() > MAX_VISIBLE {
                let rows = std::rc::Rc::new(rows);
                let list: Element = crate::VirtualList::new(
                    cx,
                    &format!("{name}.scroll"),
                    rows.len(),
                    ROW_H,
                    MAX_VISIBLE as f64 * ROW_H,
                    move |i| rows[i].clone(),
                )
                .into();
                widgets::column(vec![list])
            } else {
                widgets::column(rows)
            };
            if empty {
                let mut none = widgets::text("no matches");
                if let Some(ts) = none.text_style_mut() {
                    ts.font_size = 13.0;
                    ts.color = Color::srgb8(0x9a, 0xa1, 0xad, 0xff);
                }
                let mut pad = widgets::row(vec![none]);
                pad.style.padding = Edges::all(Dim::px(10.0));
                menu.children.push(pad);
            }
            menu.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
            menu.corner_radius = 8.0;
            menu.rare_mut().shadow = Some(crate::element::Shadow::soft());
            menu.overlay = true;
            menu.rare_mut().on_dismiss = Some(Rc::new(move |rt| open.set(rt, false)));
            menu.style.position = Position::Absolute;
            menu.style.inset = Edges {
                top: Dim::pct(1.0),
                left: Dim::px(0.0),
                ..Edges::AUTO
            };
            menu.style.margin.top = Dim::px(4.0);
            menu.style.width = Dim::px(W as f32);
            menu
        });

        Combobox {
            input: TextInput::new(cx, name, ""),
            name: name.to_string(),
            open,
            menu,
            common: Common::default(),
        }
    }
}

impl Widget for Combobox {
    fn build(self) -> Element {
        let Combobox {
            input,
            name,
            open,
            menu,
            common,
        } = self;

        let mut input: Element = input.id(format!("{name}-input")).into();
        input.style.flex_grow = 1.0;
        // Typing re-opens the list; the click focuses the editor as usual.
        {
            let prev = input.on_click.take();
            input.on_click = Some(Rc::new(move |rt| {
                if let Some(p) = &prev {
                    p(rt);
                }
                open.set(rt, true);
            }));
        }

        let mut children = vec![input];
        if let Some(menu) = menu {
            children.push(menu);
        }

        let mut el = Element {
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
        common.apply(&mut el);
        el
    }
}

impl_widget!(Combobox);
