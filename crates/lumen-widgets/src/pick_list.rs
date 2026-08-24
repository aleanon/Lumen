//! [`PickList`] — a dropdown single-select. Its `Element` (a trigger plus, when
//! open, an overlay list) is built inside [`PickList::new`]; the selection and
//! open state live in signals keyed by `name`.

use crate::widget::{impl_widget, Common, Widget};
use crate::{widgets, BuildCx, Element};
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

const W: f64 = 220.0;
const TRIGGER_H: f64 = 38.0;
const ROW_H: f64 = 34.0;
/// How many options the panel shows before it starts scrolling.
///
/// An uncapped panel is only usable while the option list is short: fifty
/// options rendered fifty rows straight off the bottom of the window, with no
/// way to reach the ones past the edge. Past this many the panel becomes a
/// windowed [`VirtualList`](crate::VirtualList) — so it costs the same whether
/// there are 20 options or 20 000, and no caller has to arrange it.
const MAX_VISIBLE: usize = 8;

/// A dropdown: the trigger shows the selection (or `placeholder`); clicking it
/// reveals the options, and choosing one stores it under `name`.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{top, PickList, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     top(cx, PickList::new(cx, "pick", "Select…", ["One", "Two", "Three"]).into())
/// }
/// # let app = App::new(build);
/// # // Rendered with the dropdown open (`pick.open`).
/// # lumen_widgets::doc_shot_open(app, 220.0, 200.0, "pick_list", "pick.open");
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
    name: String,
    placeholder: String,
    /// The current selection, read where the `BuildCx` is.
    sel: String,
    options: Vec<String>,
    selected: lumen_core::state::Signal<String>,
    open: lumen_core::state::Signal<bool>,
    /// The dropdown panel, or `None` while closed. Built eagerly for the same
    /// reason as `Combobox`'s: a long list becomes a `VirtualList`, which needs
    /// the `BuildCx`.
    menu: Option<Element>,
    common: Common,
}

/// The dropdown's disclosure chevron.
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

/// A 14 px run in `color`.
fn text(s: impl Into<crate::Text>, color: Color) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 14.0;
        ts.color = color;
    }
    e
}

impl PickList {
    /// A dropdown over `options`; the choice lives in the signal keyed by
    /// `name`.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        placeholder: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> PickList {
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
        let selected = cx.signal(name, String::new);
        let open = cx.signal(format!("{name}.open"), || false);
        let sel = selected.get(cx.runtime());
        let is_open = open.get(cx.runtime());

        let menu = is_open.then(|| {
            let row_at = {
                let options = options.clone();
                let sel = sel.clone();
                move |i: usize| {
                    let opt = options[i].clone();
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
                    r.background = if opt == sel {
                        Some(Color::srgb8(0xed, 0xf2, 0xff, 0xff))
                    } else {
                        Some(Color::srgb8(0xff, 0xff, 0xff, 0xff))
                    };
                    r.on_click = Some(Rc::new(move |rt| {
                        selected.set(rt, opt_s.clone());
                        open.set(rt, false);
                    }));
                    r
                }
            };
            let mut menu = if options.len() > MAX_VISIBLE {
                let list: Element = crate::VirtualList::new(
                    cx,
                    &format!("{name}.scroll"),
                    options.len(),
                    ROW_H,
                    MAX_VISIBLE as f64 * ROW_H,
                    row_at,
                )
                .into();
                widgets::column(vec![list])
            } else {
                widgets::column((0..options.len()).map(row_at).collect::<Vec<Element>>())
            };
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
            menu
        });

        PickList {
            name: name.to_string(),
            placeholder: placeholder.into(),
            sel,
            options,
            selected,
            open,
            menu,
            common: Common::default(),
        }
    }
}

impl Widget for PickList {
    fn build(self) -> Element {
        let PickList {
            name,
            placeholder,
            sel,
            options,
            selected,
            open,
            menu,
            common,
        } = self;

        // Trigger: current selection (or placeholder) + a chevron.
        let mut label = if sel.is_empty() {
            text(placeholder, Color::srgb8(0x9a, 0xa1, 0xad, 0xff))
        } else {
            text(sel, Color::srgb8(0x1c, 0x22, 0x30, 0xff))
        };
        label.style.flex_grow = 1.0;
        let mut trigger = widgets::row(vec![label, chevron()]);
        trigger.role = Role::Button;
        trigger.focusable = true;
        // Focus is keyed by StableId, so a focusable node needs one or it can
        // never hold focus (and never receives keys). Namespaced under `name`
        // so two dropdowns don't collide (W4).
        trigger.id = Some(format!("{name}-trigger").into());
        // `widgets::row` marks itself structural, which splices it (and its id)
        // out of the semantic tree — but this row IS the control now, so it has
        // to be visible to selectors, focus and assistive tech.
        trigger.elide_semantics = false;
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
        // W3: the WAI-ARIA combobox/listbox keys. ↑/↓ move the selection
        // directly (and open a closed list), Home/End jump to the ends, Escape
        // closes. Keyboard users no longer need the pointer to choose.
        trigger.on_key = Some(Rc::new(move |rt, ke| {
            if options.is_empty() {
                return;
            }
            let cur = selected.get(rt);
            let at = options.iter().position(|o| *o == cur);
            let pick = |rt: &lumen_core::state::Runtime, i: usize| {
                selected.set(rt, options[i].clone());
            };
            match ke.key {
                Key::Named(NamedKey::ArrowDown) => {
                    open.set(rt, true);
                    let i = match at {
                        Some(i) if i + 1 < options.len() => i + 1,
                        Some(i) => i,
                        None => 0,
                    };
                    pick(rt, i);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    open.set(rt, true);
                    let i = match at {
                        Some(i) if i > 0 => i - 1,
                        Some(i) => i,
                        None => options.len() - 1,
                    };
                    pick(rt, i);
                }
                Key::Named(NamedKey::Home) => pick(rt, 0),
                Key::Named(NamedKey::End) => pick(rt, options.len() - 1),
                Key::Named(NamedKey::Escape) => open.set(rt, false),
                _ => {}
            }
        }));

        let mut children = vec![trigger];
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

impl_widget!(PickList);
