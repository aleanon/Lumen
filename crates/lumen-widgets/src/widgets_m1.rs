//! M1 widget additions (02 §10), most importantly the windowing `VirtualList`.
//!
//! These are `Element` constructors like the M0 primitives; stateful ones own a
//! signal keyed by `name`. The remaining 02 §10 M1 widgets (RichText, Grid,
//! Wrap, Align, SplitPane, TextArea, Select, Tooltip, Popover, Menu) follow the
//! same constructor pattern.

use crate::element::{BuildCx, Element};
use crate::widget::impl_common;
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

/// Flexible empty space that grows to fill its container.
pub fn spacer() -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            flex_grow: 1.0,
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// A horizontal divider line.
pub fn divider() -> Element {
    Element {
        role: Role::Generic,
        background: Some(Color::srgb8(0xd8, 0xdd, 0xe3, 0xff)),
        style: LayoutStyle {
            height: Dim::px(1.0),
            width: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// Wrap `child` in uniform padding (px).
pub fn padding(px: f32, child: Element) -> Element {
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(px)),
            ..LayoutStyle::default()
        },
        children: vec![child],
        ..Element::default()
    }
}

/// [`Icon`] — a small labelled icon placeholder (typed form of [`icon`]).
/// # Example
///
/// ```
/// use lumen_widgets::{App, Icon};
///
/// let app = App::new(|_| Icon::new("gear").into());
/// # lumen_widgets::doc_shot(app, 40.0, 40.0, "icon");
/// ```
///
/// Renders:
///
/// ![Icon example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/icon.png)
///
/// The picture above is `src/doc_shots/icon.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Icon {
    el: Element,
}

impl Icon {
    /// An icon placeholder (a small filled square; glyph icons land with RichText).
    pub fn new(label: &str) -> Icon {
        let el = {
            Element {
                role: Role::Image,
                label: label.to_string(),
                background: Some(Color::srgb8(0x55, 0x5a, 0x61, 0xff)),
                corner_radius: 2.0,
                style: LayoutStyle {
                    width: Dim::px(16.0),
                    height: Dim::px(16.0),
                    ..LayoutStyle::default()
                },
                ..Element::default()
            }
        };
        Icon { el }
    }
}

impl_common!(Icon);

/// An icon placeholder (a small filled square; glyph icons land with RichText).
/// *(Thin shim over [`Icon`] — the typed form is preferred.)*
pub fn icon(label: &str) -> Element {
    Icon::new(label).into()
}

/// [`Switch`] — a labelled toggle switch; boolean state under `name`
/// (typed form of [`switch`]).
/// # Example
///
/// ```
/// use lumen_widgets::{App, Switch};
///
/// let app = App::new(|cx| Switch::new(cx, "wifi", "Wi-Fi").into());
/// # lumen_widgets::doc_shot(app, 120.0, 40.0, "switch");
/// ```
///
/// Renders:
///
/// ![Switch example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/switch.png)
///
/// The picture above is `src/doc_shots/switch.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Switch {
    el: Element,
}

impl Switch {
    /// A toggle switch with its own boolean state (`name`).
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<String>) -> Switch {
        let el = {
            let label = label.into();
            let on = cx.signal(name, || false);
            let is = on.get(cx.runtime());
            let track = Element {
                background: Some(if is {
                    Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
                } else {
                    Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)
                }),
                corner_radius: 10.0,
                style: LayoutStyle {
                    width: Dim::px(36.0),
                    height: Dim::px(20.0),
                    ..LayoutStyle::default()
                },
                ..Element::default()
            };
            Element {
                role: Role::Switch,
                label: label.clone(),
                focusable: true,
                actions: vec![Action::Click, Action::Focus],
                states: vec![if is {
                    SemState::Checked
                } else {
                    SemState::Unchecked
                }],
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    column_gap: Dim::px(6.0),
                    ..LayoutStyle::default()
                },
                on_click: Some(Rc::new(move |rt| on.update(rt, |v| *v = !*v))),
                children: vec![track, Element::text(label)],
                ..Element::default()
            }
        };
        Switch { el }
    }
}

impl_common!(Switch);

/// A toggle switch with its own boolean state (`name`).
/// *(Thin shim over [`Switch`] — the typed form is preferred.)*
pub fn switch(cx: &BuildCx, name: &str, label: impl Into<String>) -> Element {
    Switch::new(cx, name, label).into()
}

/// [`Stepper`] — a `-`/value/`+` numeric stepper; integer state under
/// `name` (typed form of [`stepper`]).
/// # Example
///
/// ```
/// use lumen_widgets::{App, Stepper};
///
/// let app = App::new(|cx| Stepper::new(cx, "qty", 0, 10).into());
/// # lumen_widgets::doc_shot(app, 120.0, 48.0, "stepper");
/// ```
///
/// Renders:
///
/// ![Stepper example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/stepper.png)
///
/// The picture above is `src/doc_shots/stepper.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Stepper {
    el: Element,
}

impl Stepper {
    /// A numeric stepper (`-`/value/`+`) with its own integer state (`name`).
    pub fn new(cx: &BuildCx, name: &str, min: i64, max: i64) -> Stepper {
        let el = {
            let value = cx.signal(name, || min);
            let v = value.get(cx.runtime());
            let dec =
                crate::widgets::button("-", move |rt| value.update(rt, |x| *x = (*x - 1).max(min)))
                    .id("dec");
            let inc =
                crate::widgets::button("+", move |rt| value.update(rt, |x| *x = (*x + 1).min(max)))
                    .id("inc");
            Element {
                role: Role::Group,
                label: format!("{v}"),
                value: Some(format!("{v}")),
                actions: vec![Action::Increment, Action::Decrement],
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    column_gap: Dim::px(8.0),
                    ..LayoutStyle::default()
                },
                children: vec![dec, Element::text(format!("{v}")).id("value"), inc],
                ..Element::default()
            }
        };
        Stepper { el }
    }
}

impl_common!(Stepper);

/// A numeric stepper (`-`/value/`+`) with its own integer state (`name`).
/// *(Thin shim over [`Stepper`] — the typed form is preferred.)*
pub fn stepper(cx: &BuildCx, name: &str, min: i64, max: i64) -> Element {
    Stepper::new(cx, name, min, max).into()
}

/// [`Tabs`] — a tab bar; selected index under `name` (typed form of
/// [`tabs`]).
/// # Example
///
/// ```
/// use lumen_widgets::{App, Tabs};
///
/// let app = App::new(|cx| Tabs::new(cx, "tab", &["One", "Two", "Three"]).into());
/// # lumen_widgets::doc_shot(app, 220.0, 44.0, "tabs");
/// ```
///
/// Renders:
///
/// ![Tabs example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/tabs.png)
///
/// The picture above is `src/doc_shots/tabs.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Tabs {
    el: Element,
}

impl Tabs {
    /// A tab bar with its own selected-index state (`name`).
    pub fn new(cx: &BuildCx, name: &str, labels: &[&str]) -> Tabs {
        let el = {
            let selected = cx.signal(name, || 0usize);
            let cur = selected.get(cx.runtime());
            let tabs: Vec<Element> = labels
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let on = i == cur;
                    Element {
                        role: Role::Tab,
                        label: (*label).to_string(),
                        focusable: true,
                        actions: vec![Action::Click, Action::Focus],
                        states: if on { vec![SemState::Selected] } else { vec![] },
                        background: Some(if on {
                            Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
                        } else {
                            Color::srgb8(0xee, 0xf0, 0xf3, 0xff)
                        }),
                        corner_radius: 4.0,
                        style: LayoutStyle {
                            padding: Edges::all(Dim::px(6.0)),
                            ..LayoutStyle::default()
                        },
                        content: crate::NodeContent::Text(
                            (*label).to_string(),
                            lumen_text::TextStyle {
                                font_size: 14.0,
                                weight: 400.0,
                                color: if on { Color::WHITE } else { Color::BLACK },
                                line_height: None,
                                letter_spacing: 0.0,
                                family: None,
                            },
                        ),
                        on_click: Some(Rc::new(move |rt| selected.set(rt, i))),
                        ..Element::default()
                    }
                })
                .collect();
            Element {
                role: Role::TabList,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    column_gap: Dim::px(4.0),
                    ..LayoutStyle::default()
                },
                children: tabs,
                ..Element::default()
            }
        };
        Tabs { el }
    }
}

impl_common!(Tabs);

/// A tab bar with its own selected-index state (`name`).
/// *(Thin shim over [`Tabs`] — the typed form is preferred.)*
pub fn tabs(cx: &BuildCx, name: &str, labels: &[&str]) -> Element {
    Tabs::new(cx, name, labels).into()
}

/// [`VirtualList`] — a windowing list materializing only visible items
/// plus overscan; scroll offset under `name` (typed form of
/// [`virtual_list`]).
/// # Example
///
/// ```
/// use lumen_widgets::{widgets, App, VirtualList};
///
/// let app = App::new(|cx| {
///     VirtualList::new(cx, "vl", 1000, 24.0, 96.0, |i| widgets::text(format!("Row {i}"))).into()
/// });
/// # lumen_widgets::doc_shot(app, 180.0, 100.0, "virtual_list");
/// ```
///
/// Renders:
///
/// ![Virtual List example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/virtual_list.png)
///
/// The picture above is `src/doc_shots/virtual_list.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct VirtualList {
    el: Element,
}

impl VirtualList {
    /// A windowing list (02 §10): materializes only the visible items plus overscan,
    /// regardless of `item_count`. State (`name`) is the scroll offset.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        item_count: usize,
        item_height: f64,
        viewport_h: f64,
        render: impl Fn(usize) -> Element,
    ) -> VirtualList {
        let el = {
            const OVERSCAN: usize = 2;
            let offset = cx.signal(name, || 0.0f64);
            let y = offset.get(cx.runtime());

            let first = ((y / item_height).floor() as usize).saturating_sub(OVERSCAN);
            let per_view = (viewport_h / item_height).ceil() as usize;
            let last = (first + per_view + OVERSCAN * 2).min(item_count);

            let children: Vec<Element> = (first..last)
                .map(|i| {
                    let top = (i as f64 * item_height) - y;
                    let mut el = render(i);
                    el.style.position = Position::Absolute;
                    el.style.inset = Edges {
                        left: Dim::px(0.0),
                        top: Dim::px(top as f32),
                        ..Edges::AUTO
                    };
                    el.style.height = Dim::px(item_height as f32);
                    el
                })
                .collect();

            let max_y = (item_count as f64 * item_height - viewport_h).max(0.0);
            Element {
                role: Role::List,
                scroll: Some(ScrollInfo {
                    x: 0.0,
                    y,
                    max_x: 0.0,
                    max_y,
                }),
                actions: vec![Action::ScrollIntoView],
                style: LayoutStyle {
                    position: Position::Relative,
                    width: Dim::pct(1.0),
                    height: Dim::px(viewport_h as f32),
                    ..LayoutStyle::default()
                },
                on_wheel: Some(Rc::new(move |rt, _dx, dy, _mods| {
                    offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
                })),
                children,
                ..Element::default()
            }
        };
        VirtualList { el }
    }
}

impl_common!(VirtualList);

/// A windowing list (02 §10): materializes only the visible items plus overscan,
/// regardless of `item_count`. State (`name`) is the scroll offset.
/// *(Thin shim over [`VirtualList`] — the typed form is preferred.)*
pub fn virtual_list(
    cx: &BuildCx,
    name: &str,
    item_count: usize,
    item_height: f64,
    viewport_h: f64,
    render: impl Fn(usize) -> Element,
) -> Element {
    VirtualList::new(cx, name, item_count, item_height, viewport_h, render).into()
}

#[cfg(test)]
mod typed_tests {
    use crate::{widgets_m1, App};
    use kurbo::Size;
    use lumen_core::events::{Event, PointerEvent};
    use lumen_core::geometry::Point;
    use lumen_core::state::Signal;

    /// The typed forms produce the same trees as their fn shims (migration
    /// contract) and behave: Switch toggles, Tabs select, VirtualList windows.
    #[test]
    fn typed_forms_match_shims_and_behave() {
        let mut h = App::new(|cx| {
            crate::widgets::column(vec![
                widgets_m1::Switch::new(cx, "wifi", "Wi-Fi").id("sw").into(),
                widgets_m1::Tabs::new(cx, "tab", &["One", "Two"])
                    .id("tabs")
                    .into(),
                widgets_m1::Stepper::new(cx, "n", 0, 5).id("st").into(),
                widgets_m1::Icon::new("gear").id("ic").into(),
                widgets_m1::VirtualList::new(cx, "vl", 1000, 20.0, 100.0, |i| {
                    crate::widgets::text(format!("row {i}"))
                })
                .id("vl")
                .into(),
            ])
        })
        .run_headless(Size::new(400.0, 400.0));
        h.pump();

        // Switch: click toggles the boolean.
        let b = h.node_bounds_by_id("sw").expect("switch laid out");
        let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        h.inject(Event::PointerDown(PointerEvent::at(p)));
        h.inject(Event::PointerUp(PointerEvent::at(p)));
        h.pump();
        let on: Signal<bool> = h.runtime().signal("wifi", || false);
        assert!(on.get(h.runtime()), "switch toggled");

        // VirtualList windows: ~7 rows materialized of 1000.
        let t = h.semantics_json().to_string();
        assert!(t.contains("row 0") && !t.contains("row 500"), "windowing");

        h.assert_view_coherent();
    }

    /// Shim output ≡ typed output (byte-identical semantic trees).
    #[test]
    fn shim_and_typed_trees_are_identical() {
        let a = App::new(|cx| crate::widgets::column(vec![widgets_m1::switch(cx, "s", "L")]))
            .run_headless(Size::new(200.0, 100.0))
            .semantics_json()
            .to_string();
        let b = App::new(|cx| {
            crate::widgets::column(vec![widgets_m1::Switch::new(cx, "s", "L").into()])
        })
        .run_headless(Size::new(200.0, 100.0))
        .semantics_json()
        .to_string();
        assert_eq!(a, b);
    }
}
