//! [`Scrollable`] — a self-stateful vertical scroll container. Its `Element` is
//! built inside [`Scrollable::new`]; the offset lives in a signal keyed by
//! `name`. (For very long lists, virtualize — this lays out all children.)

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::{Action, Role, ScrollInfo};
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
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
        let max_y = (content_h - viewport_h).max(0.0);
        // Apply the stored offset clamped to the *current* extent: content
        // can shrink between builds (a tab switch, a filter), and a stale
        // offset must not push what's left out of the viewport.
        let y = offset.get(cx.runtime()).clamp(0.0, max_y);

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
                // Anchors the absolutely-positioned overlay scrollbar.
                position: Position::Relative,
                height: Dim::px(viewport_h as f32),
                ..LayoutStyle::default()
            },
            // Positive wheel delta scrolls toward the end (the shell normalizes
            // the OS sign so wheel-down moves the content down).
            on_wheel: Some(Rc::new(move |rt, _dx, dy, _mods| {
                offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
            })),
            // W3: keyboard scrolling. Focusable so Tab can reach the viewport —
            // a scroll region a keyboard user cannot move is a real a11y gap
            // (WAI-ARIA: a scrollable region must be keyboard operable).
            focusable: true,
            on_key: Some(Rc::new(move |rt, ke| {
                let line = 40.0;
                let page = (viewport_h - line).max(line);
                let step = match ke.key {
                    Key::Named(NamedKey::ArrowDown) => line,
                    Key::Named(NamedKey::ArrowUp) => -line,
                    Key::Named(NamedKey::PageDown) => page,
                    Key::Named(NamedKey::PageUp) => -page,
                    Key::Named(NamedKey::Home) => {
                        offset.set(rt, 0.0);
                        return;
                    }
                    Key::Named(NamedKey::End) => {
                        offset.set(rt, max_y);
                        return;
                    }
                    _ => return,
                };
                offset.update(rt, |o| *o = (*o + step).clamp(0.0, max_y));
            })),
            children: match overlay_scrollbar(viewport_h, y, max_y, offset) {
                Some(bar) => vec![inner, bar],
                None => vec![inner],
            },
            ..Element::default()
        };
        Scrollable { el }
    }
}

impl_common!(Scrollable);

/// An overlay scrollbar for any container that reports [`ScrollInfo`].
///
/// **Overlay**, so it is absolutely positioned over the content's trailing edge
/// and costs no layout width — turning it on reflows nothing.
///
/// The thumb is sized from the container's *reported* extent, not from its laid
/// out children, which is what makes it correct for a virtualized list.
/// `VirtualList` computes `max_y` from `item_count * item_height`, so the true
/// content height is known even though only a window of rows exists — dragging
/// to the middle lands in the middle on the first try. A scrollbar derived from
/// materialized content instead (the usual approach, and what makes virtual
/// lists feel broken elsewhere) grows as rows appear, so the thumb keeps
/// shrinking out from under the pointer.
///
/// Returns `None` when there is nothing to scroll: an affordance that cannot
/// move is noise, and it would also cover content.
///
/// The drag lives on the **track**, not the thumb, because `apply_drag` reports
/// the pointer as a fraction of the *handler's own* node — so `frac_y` is
/// already "how far down the track", with no window-coordinate arithmetic and
/// no drag-origin state. It also makes clicking the track jump there, which is
/// the behaviour being asked for.
pub(crate) fn overlay_scrollbar(
    viewport_h: f64,
    y: f64,
    max_y: f64,
    offset: lumen_core::state::Signal<f64>,
) -> Option<Element> {
    if max_y <= 0.5 || viewport_h <= 0.0 {
        return None;
    }
    const TRACK_W: f64 = 8.0;
    const MIN_THUMB: f64 = 24.0;
    let content_h = viewport_h + max_y;
    // Proportional, with a floor so a very long list still leaves something
    // grabbable.
    let thumb_h = ((viewport_h / content_h) * viewport_h)
        .max(MIN_THUMB)
        .min(viewport_h);
    let travel = (viewport_h - thumb_h).max(0.0);
    let pos_frac = (y / max_y).clamp(0.0, 1.0);

    let thumb = Element {
        background: Some(lumen_core::Color::srgb8(0x5f, 0x63, 0x68, 0xb0)),
        corner_radius: (TRACK_W - 2.0) / 2.0,
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(1.0),
                top: Dim::px((pos_frac * travel) as f32),
                ..Edges::AUTO
            },
            width: Dim::px((TRACK_W - 2.0) as f32),
            height: Dim::px(thumb_h as f32),
            ..LayoutStyle::default()
        },
        ..Element::default()
    };

    Some(Element {
        // No `ScrollBar` role exists; `Slider` is the closest — ARIA models a
        // scrollbar as a range widget — and it keeps the bar drivable by the
        // agent rather than invisible to it. A dedicated role would be more
        // precise if this proves confusing to a screen reader.
        role: Role::Slider,
        label: "Scrollbar".to_string(),
        background: Some(lumen_core::Color::srgb8(0x00, 0x00, 0x00, 0x14)),
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                right: Dim::px(0.0),
                top: Dim::px(0.0),
                ..Edges::AUTO
            },
            width: Dim::px(TRACK_W as f32),
            height: Dim::px(viewport_h as f32),
            ..LayoutStyle::default()
        },
        on_drag: Some(Rc::new(move |rt, _fx, fy, _pos| {
            offset.set(rt, (fy * max_y).clamp(0.0, max_y));
        })),
        children: vec![thumb],
        ..Element::default()
    })
}
