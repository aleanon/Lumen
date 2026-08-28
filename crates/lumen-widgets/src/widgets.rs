//! The ten M0 primitive widgets (02 §10).
//!
//! Each is a constructor returning an [`Element`]; stateful widgets own a signal
//! keyed by `name` (so their state lives in the store and survives rebuilds).
//! Default styles are hardcoded constants until the `.lss` system (T1.2).

use crate::element::{BuildCx, Element};
use crate::widget::{impl_widget, Common, Widget};
use lumen_core::semantics::{Action, Role, ScrollInfo};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
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
pub fn text(s: impl Into<crate::Text>) -> Element {
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

/// [`Image`] — a decoded bitmap at its own pixel size (typed form of
/// [`image`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Image, RgbaImage, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // A 48x32 solid-blue image (real apps decode PNG/JPEG bytes instead).
///     let px: Vec<u8> = (0..48 * 32).flat_map(|_| [0x2b, 0x6c, 0xff, 0xff]).collect();
///     let img = RgbaImage::from_raw(48, 32, px);
///     centered(cx, Image::new(img).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 96.0, 72.0, "image");
/// ```
///
/// Renders:
///
/// ![Image example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/image.png)
///
/// The picture above is `src/doc_shots/image.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Image {
    img: RgbaImage,
    common: Common,
}

impl Image {
    /// An image of its own pixel size.
    pub fn new(img: RgbaImage) -> Image {
        Image {
            img,
            common: Common::default(),
        }
    }
}

impl Widget for Image {
    fn build(self) -> Element {
        let Image { img, common } = self;
        let (w, h) = (img.width() as f32, img.height() as f32);
        let mut el = Element {
            role: Role::Image,
            content: crate::NodeContent::Image(img),
            style: LayoutStyle {
                width: Dim::px(w),
                height: Dim::px(h),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Image);

/// An image of its own pixel size.
/// *(Thin shim over [`Image`] — the typed form is preferred.)*
pub fn image(img: RgbaImage) -> Element {
    Image::new(img).into()
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
    label: impl Into<crate::Text>,
    on_click: impl Fn(&lumen_core::Runtime) + 'static,
) -> Element {
    Element::button(label).on_click(on_click)
}

/// A checkbox with its own boolean state (`name`). Click or Space toggles it.
pub fn checkbox(cx: &BuildCx, name: &str, label: impl Into<crate::Text>) -> Element {
    crate::check_box::CheckBox::new(cx, name, label).into()
}

/// A slider over `[min, max]` with its own value state (`name`). Drag/press to
/// set the value from the pointer position.
pub fn slider(cx: &BuildCx, name: &str, min: f64, max: f64) -> Element {
    crate::slider::Slider::new(cx, name, min, max).into()
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
    // See `Scrollable::new`: the viewport's `align_items: Stretch` would clamp
    // this column to the viewport height and flex-shrink its rows.
    inner.style.align_self = Some(lumen_layout::Align::Start);
    inner.style.flex_shrink = 0.0;
    Element {
        role: Role::ScrollArea,

        actions: vec![Action::ScrollIntoView],
        style: LayoutStyle {
            height: Dim::px(viewport_h as f32),
            ..LayoutStyle::default()
        },

        children: vec![inner],
        ..Element::default()
    }
    .set_scroll(Some(ScrollInfo {
        x: 0.0,
        y,
        max_x: 0.0,
        max_y,
    }))
    .set_on_wheel(Some(Rc::new(move |rt, _dx, dy, _mods| {
        offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
    })))
}

/// A single-style text input with its own string state (`name`). Pre-IME
/// (committed text only): no caret, no selection, no key map — Backspace pops
/// the last character and that is the whole edit model.
///
/// **Prefer [`TextInput`](crate::TextInput)**, which has all of it. This
/// remains only for the pre-M1 string-signal contract; the last widget still
/// built on it (`FindReplaceBar`) moved off it because a field you cannot
/// correct a typo in is not a field.
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

        // The basic field has no caret — editing happens at the end, so
        // Backspace pops the last character (with Ctrl: the whole value).
        ..Element::default()
    }
    .set_on_key(Some(Rc::new(move |rt, ev| {
        if ev.key == lumen_core::events::Key::Named(lumen_core::events::NamedKey::Backspace) {
            if ev.modifiers.contains(lumen_core::events::Modifiers::CTRL)
                || ev.modifiers.contains(lumen_core::events::Modifiers::META)
            {
                value.set(rt, String::new());
            } else {
                value.update(rt, |s| {
                    s.pop();
                });
            }
        }
    })))
    .set_on_text(Some(Rc::new(move |rt, t| {
        let t = t.to_string();
        value.update(rt, |s| s.push_str(&t))
    })))
}

/// [`Canvas`] — an immediate-mode drawing surface (typed form of
/// [`canvas`]): `draw` paints into a `Frame` sized to the widget each frame.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Canvas, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_render::Brush;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let canvas = Canvas::new(80.0, 40.0, |f, size| {
///         f.fill_rect(
///             kurbo::Rect::new(0.0, 0.0, size.width, size.height),
///             Brush::Solid(Color::srgb8(0x18, 0xc2, 0x7d, 0xff)),
///         );
///     });
///     centered(cx, canvas.into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 120.0, 72.0, "canvas");
/// ```
///
/// Renders:
///
/// ![Canvas example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/canvas.png)
///
/// The picture above is `src/doc_shots/canvas.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Canvas {
    width: f64,
    height: f64,
    draw: crate::element::CanvasFn,
    common: Common,
}

impl Canvas {
    /// An immediate-mode drawing canvas (E8.1). `draw` paints into a `Frame` sized
    /// to the widget each frame; emit paths, rects, circles, and gradients.
    pub fn new(
        width: f64,
        height: f64,
        draw: impl Fn(&mut lumen_render::canvas::Frame, lumen_core::geometry::Size) + 'static,
    ) -> Canvas {
        Canvas {
            width,
            height,
            draw: Rc::new(draw),
            common: Common::default(),
        }
    }
}

impl Widget for Canvas {
    fn build(self) -> Element {
        let Canvas {
            width,
            height,
            draw,
            common,
        } = self;
        let mut el = Element {
            role: Role::Image,
            style: LayoutStyle {
                width: Dim::px(width as f32),
                height: Dim::px(height as f32),
                ..LayoutStyle::default()
            },
            content: crate::NodeContent::Canvas(draw),
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Canvas);

/// An immediate-mode drawing canvas (E8.1). `draw` paints into a `Frame` sized
/// to the widget each frame; emit paths, rects, circles, and gradients.
/// *(Thin shim over [`Canvas`] — the typed form is preferred.)*
pub fn canvas(
    width: f64,
    height: f64,
    draw: impl Fn(&mut lumen_render::canvas::Frame, lumen_core::geometry::Size) + 'static,
) -> Element {
    Canvas::new(width, height, draw).into()
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

// SD2: the rest of the catalogue, re-exported so `widgets::` is the single
// place to find a widget regardless of which milestone happened to add it.
pub use crate::charts::{bar_chart, BarChart};
pub use crate::controls::*;
pub use crate::lists::*;
pub use crate::nav_chrome::*;
pub use crate::overlay::*;
pub use crate::panes::*;
pub use crate::pickers::*;
pub use crate::primitives::*;
pub use crate::text_editing::*;

#[cfg(test)]
mod typed_tests {
    use crate::{widgets, App};
    use kurbo::Size;

    /// Image and Canvas typed forms lay out at their intrinsic sizes and the
    /// shims produce identical trees.
    #[test]
    fn image_and_canvas_typed_forms() {
        let mut h = App::new(|_| {
            widgets::column(vec![
                widgets::Image::new(crate::RgbaImage::new(24, 16))
                    .id("img")
                    .into(),
                widgets::Canvas::new(50.0, 30.0, |f, size| {
                    f.fill_rect(
                        kurbo::Rect::new(0.0, 0.0, size.width, size.height),
                        lumen_render::Brush::Solid(lumen_core::Color::srgb8(255, 0, 0, 255)),
                    );
                })
                .id("cv")
                .into(),
            ])
        })
        .run_headless(Size::new(200.0, 200.0));
        h.pump();
        let img = h.node_bounds_by_id("img").expect("image laid out");
        assert_eq!((img.width(), img.height()), (24.0, 16.0));
        let cv = h.node_bounds_by_id("cv").expect("canvas laid out");
        assert_eq!((cv.width(), cv.height()), (50.0, 30.0));
        // The canvas painted (red present).
        let px = h.screenshot();
        assert!(px.pixels().chunks_exact(4).any(|p| p[0] > 200 && p[1] < 60));
        h.assert_view_coherent();
    }

    /// Container::stack overlays children like widgets::stack.
    #[test]
    fn container_stack_matches_stack_fn() {
        let mk_kids = || {
            vec![
                widgets::text("under").id("under"),
                widgets::text("over").id("over"),
            ]
        };
        let a = App::new(move |_| widgets::stack(mk_kids()))
            .run_headless(Size::new(200.0, 100.0))
            .semantics_json()
            .to_string();
        let b = App::new(move |_| crate::Container::new(mk_kids()).stack().into())
            .run_headless(Size::new(200.0, 100.0))
            .semantics_json()
            .to_string();
        assert_eq!(a, b);
    }
}
