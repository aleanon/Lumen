//! [`Scrollable`] — a self-stateful vertical scroll container. Its `Element` is
//! built inside [`Scrollable::new`]; the offset lives in a signal keyed by
//! `name`. (For very long lists, virtualize — this lays out all children.)

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, ScrollInfo};
use lumen_layout::{Dim, LayoutStyle};
use std::rc::Rc;

/// A clipping viewport that scrolls its content vertically with the wheel.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Scrollable, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let content = vec![widgets::text("Tall content")];
///     centered(cx, Scrollable::new(cx, "sc", 80.0, 400.0, content).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 120.0, "scrollable");
/// ```
///
/// Renders:
///
/// ![Scrollable example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/scrollable.png)
///
/// The picture above is `src/doc_shots/scrollable.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Scrollable {
    el: Element,
}

impl Scrollable {
    /// A `viewport_h`-tall viewport over `content_h` of `children`, offset stored
    /// under `name`.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        viewport_h: f64,
        content_h: f64,
        children: impl Into<Vec<Element>>,
    ) -> Scrollable {
        let offset = cx.signal(name, || 0.0f64);
        let y = offset.get(cx.runtime());
        let max_y = (content_h - viewport_h).max(0.0);

        let mut inner = Element::column(children);
        inner.style.margin.top = Dim::px(-(y as f32));
        // Fill the viewport width so rows can right-align (flex_grow) within it.
        inner.style.width = Dim::pct(1.0);

        let el = Element {
            role: Role::ScrollArea,
            clip: true, // overflow:hidden — content outside the viewport is masked
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
            // Positive wheel delta scrolls toward the end (the shell normalizes
            // the OS sign so wheel-down moves the content down).
            on_wheel: Some(Rc::new(move |rt, _dx, dy, _mods| {
                offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
            })),
            children: vec![inner],
            ..Element::default()
        };
        Scrollable { el }
    }
}

impl_common!(Scrollable);
