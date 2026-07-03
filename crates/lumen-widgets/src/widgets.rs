//! The ten M0 primitive widgets (02 §10).
//!
//! Each is a constructor returning an [`Element`]; stateful widgets own a signal
//! keyed by `name` (so their state lives in the store and survives rebuilds).
//! Default styles are hardcoded constants until the `.lss` system (T1.2).

use crate::element::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, ScrollInfo, State as SemState};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_render::RgbaImage;
use lumen_text::TextStyle;
use std::rc::Rc;

/// A keyed reactive list (F5.1): render each item inside its own `cx.scope`
/// identified by `key(item)`, so reordering reuses each item's cached subtree
/// (F1) and only added/removed/changed items re-run. The **stable key** (not the
/// index) is the identity — capture it, not a position, in handlers. Vanished
/// keys are swept after the build (their cache + scope-local signals freed), so a
/// churning list stays memory-bounded. Keys must be unique within the list.
///
/// ```ignore
/// let rows = widgets::keyed(cx, todos, |t| t.id.to_string(), |cx, t| row_view(cx, t));
/// widgets::column(rows)
/// ```
pub fn keyed<T>(
    cx: &mut BuildCx,
    items: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> String,
    view: impl Fn(&mut BuildCx, &T) -> Element,
) -> Vec<Element> {
    items
        .into_iter()
        .map(|item| {
            let k = key(&item);
            cx.scope(&k, |cx| view(cx, &item))
        })
        .collect()
}

/// Static text.
pub fn text(s: impl Into<String>) -> Element {
    Element::text(s)
}

/// Mount a custom leaf widget (E2): a third-party / agent-authored
/// [`crate::LeafWidget`] becomes a first-class node — measured, painted, and
/// given semantics (its role/label) by the runtime, just like a built-in leaf.
pub fn leaf(w: impl crate::LeafWidget + 'static) -> Element {
    let (role, label) = w.semantics();
    Element {
        role,
        label,
        content: crate::NodeContent::Custom(Rc::new(w)),
        ..Element::default()
    }
}

/// An image of its own pixel size.
pub fn image(img: RgbaImage) -> Element {
    let (w, h) = (img.width() as f32, img.height() as f32);
    Element {
        role: Role::Image,
        content: crate::NodeContent::Image(img),
        style: LayoutStyle {
            width: Dim::px(w),
            height: Dim::px(h),
            ..LayoutStyle::default()
        },
        ..Element::default()
    }
}

/// A horizontal flex container.
pub fn row(children: impl Into<Vec<Element>>) -> Element {
    Element::row(children)
}

/// A vertical flex container.
pub fn column(children: impl Into<Vec<Element>>) -> Element {
    Element::column(children)
}

/// A z-stack: children overlaid at the top-left, last on top.
pub fn stack(children: impl Into<Vec<Element>>) -> Element {
    let kids = children
        .into()
        .into_iter()
        .map(|mut c| {
            c.style.position = Position::Absolute;
            c.style.inset = Edges {
                left: Dim::px(0.0),
                top: Dim::px(0.0),
                ..Edges::AUTO
            };
            c
        })
        .collect();
    Element {
        role: Role::Group,
        elide_semantics: true,
        style: LayoutStyle {
            position: Position::Relative,
            ..LayoutStyle::default()
        },
        children: kids,
        ..Element::default()
    }
}

/// A push button.
pub fn button(
    label: impl Into<String>,
    on_click: impl Fn(&lumen_core::Runtime) + 'static,
) -> Element {
    Element::button(label).on_click(on_click)
}

/// A checkbox with its own boolean state (`name`). Click or Space toggles it.
pub fn checkbox(cx: &BuildCx, name: &str, label: impl Into<String>) -> Element {
    let label = label.into();
    let checked = cx.signal(name, || false);
    let is = checked.get(cx.runtime());
    let box_color = if is {
        Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
    } else {
        Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)
    };
    let boxel = Element {
        background: Some(box_color),
        corner_radius: 3.0,
        style: LayoutStyle {
            width: Dim::px(20.0),
            height: Dim::px(20.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };
    Element {
        role: Role::Checkbox,
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
        on_click: Some(Rc::new(move |rt| checked.update(rt, |c| *c = !*c))),
        children: vec![boxel, Element::text(label)],
        ..Element::default()
    }
}

/// A slider over `[min, max]` with its own value state (`name`). Drag/press to
/// set the value from the pointer position.
pub fn slider(cx: &BuildCx, name: &str, min: f64, max: f64) -> Element {
    let value = cx.signal(name, || min);
    let v = value.get(cx.runtime());
    let frac = ((v - min) / (max - min)).clamp(0.0, 1.0);
    const W: f64 = 200.0;
    const THUMB: f64 = 16.0;
    let track = Element {
        background: Some(Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)),
        corner_radius: 2.0,
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                top: Dim::px(8.0),
                ..Edges::AUTO
            },
            width: Dim::px(W as f32),
            height: Dim::px(4.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };
    let thumb = Element {
        background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
        corner_radius: (THUMB / 2.0),
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px((frac * (W - THUMB)) as f32),
                top: Dim::px(0.0),
                ..Edges::AUTO
            },
            width: Dim::px(THUMB as f32),
            height: Dim::px(THUMB as f32),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };
    Element {
        role: Role::Slider,
        focusable: true,
        value: Some(format!("{v:.0}")),
        actions: vec![Action::SetValue, Action::Increment, Action::Decrement],
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::px(W as f32),
            height: Dim::px(THUMB as f32),
            ..LayoutStyle::default()
        },
        on_drag: Some(Rc::new(move |rt, f, _, _| {
            value.set(rt, min + f * (max - min))
        })),
        children: vec![track, thumb],
        ..Element::default()
    }
}

/// A scroll container with its own vertical offset state (`name`). The wheel
/// scrolls the content and updates `scroll` in semantics.
pub fn scroll(
    cx: &BuildCx,
    name: &str,
    viewport_h: f64,
    content_h: f64,
    children: impl Into<Vec<Element>>,
) -> Element {
    let offset = cx.signal(name, || 0.0f64);
    let y = offset.get(cx.runtime());
    let max_y = (content_h - viewport_h).max(0.0);
    let mut inner = Element::column(children);
    inner.style.margin.top = Dim::px(-(y as f32));
    Element {
        role: Role::ScrollArea,
        scroll: Some(ScrollInfo {
            x: 0.0,
            y,
            max_x: 0.0,
            max_y,
        }),
        actions: vec![Action::ScrollIntoView],
        style: LayoutStyle {
            height: Dim::px(viewport_h as f32),
            ..LayoutStyle::default()
        },
        on_wheel: Some(Rc::new(move |rt, _dx, dy| {
            offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
        })),
        children: vec![inner],
        ..Element::default()
    }
}

/// A single-style text input with its own string state (`name`). Pre-IME
/// (committed text only); full IME lands in M1.
pub fn text_field_basic(cx: &BuildCx, name: &str, initial: &str) -> Element {
    let value = cx.signal(name, || initial.to_string());
    let v = value.get(cx.runtime());
    let shown = if v.is_empty() {
        " ".to_string()
    } else {
        v.clone()
    };
    Element {
        role: Role::TextInput,
        focusable: true,
        label: v.clone(),
        value: Some(v),
        actions: vec![Action::Focus, Action::SetValue],
        background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
        corner_radius: 4.0,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(6.0)),
            min_width: Dim::px(120.0),
            ..LayoutStyle::default()
        },
        content: crate::NodeContent::Text(shown, TextStyle::default()),
        on_text: Some(Rc::new(move |rt, t| {
            let t = t.to_string();
            value.update(rt, |s| s.push_str(&t))
        })),
        ..Element::default()
    }
}

/// An immediate-mode drawing canvas (E8.1). `draw` paints into a `Frame` sized
/// to the widget each frame; emit paths, rects, circles, and gradients.
pub fn canvas(
    width: f64,
    height: f64,
    draw: impl Fn(&mut lumen_render::canvas::Frame, lumen_core::geometry::Size) + 'static,
) -> Element {
    use lumen_layout::{Dim, LayoutStyle};
    Element {
        role: lumen_core::semantics::Role::Image,
        style: LayoutStyle {
            width: Dim::px(width as f32),
            height: Dim::px(height as f32),
            ..LayoutStyle::default()
        },
        content: crate::NodeContent::Canvas(std::rc::Rc::new(draw)),
        ..Element::default()
    }
}

/// A determinate progress bar showing `fraction` (0..=1) of a track filled.
pub fn progress_bar(fraction: f64) -> Element {
    use lumen_layout::{Dim, LayoutStyle};
    let frac = fraction.clamp(0.0, 1.0);
    let fill = Element {
        role: lumen_core::semantics::Role::Generic,
        background: Some(lumen_core::Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
        style: LayoutStyle {
            width: Dim::pct(frac as f32),
            height: Dim::pct(1.0),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };
    Element {
        role: lumen_core::semantics::Role::Progress,
        value: Some(format!("{:.0}%", frac * 100.0)),
        background: Some(lumen_core::Color::srgb8(0xe0, 0xe2, 0xe6, 0xff)),
        corner_radius: 4.0,
        style: LayoutStyle {
            width: Dim::px(200.0),
            height: Dim::px(12.0),
            ..LayoutStyle::default()
        },
        children: vec![fill],
        ..Element::default()
    }
}
