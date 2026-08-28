//! [`Scrollable`] — a self-stateful vertical scroll container. Its `Element` is
//! built inside [`Scrollable::new`]; the offset lives in a signal keyed by
//! `name`. (For very long lists, virtualize — this lays out all children.)

use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Element};
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::{Action, Role, ScrollInfo};
use lumen_layout::{Align, Dim, Edges, LayoutStyle, Position};
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
    name: String,
    viewport_h: f64,
    children: Vec<Element>,
    /// The clamped scroll offset and the extent, resolved where the `BuildCx`
    /// is.
    y: f64,
    max_y: f64,
    offset: lumen_core::state::Signal<f64>,
    common: Common,
}

impl Scrollable {
    /// A clipped viewport `viewport_h` px tall over `content_h` px of content.
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
        Scrollable {
            name: name.to_string(),
            viewport_h,
            children: children.into(),
            y,
            max_y,
            offset,
            common: Common::default(),
        }
    }
}

impl Widget for Scrollable {
    fn build(self) -> Element {
        let Scrollable {
            name,
            viewport_h,
            children,
            y,
            max_y,
            offset,
            common,
        } = self;

        let mut inner = Element::column(children);
        inner.style.margin.top = Dim::px(-(y as f32));
        // Fill the viewport width so rows can right-align (flex_grow) within it.
        inner.style.width = Dim::pct(1.0);
        // …but NOT the viewport height. The viewport is a flex container, and
        // its default `align_items: Stretch` was clamping this column to the
        // viewport's cross size — so a column of 30 px rows taller than the
        // viewport had its rows flex-shrunk to fit, and they grew back toward
        // 30 px as scrolling pushed `margin-top` negative and the box longer.
        // Rows visibly changed height while you scrolled. `align_self: Start`
        // lets the column size to its content, which is what a scroll surface
        // is for.
        inner.style.align_self = Some(Align::Start);
        // A scroll surface never shrinks its content to fit — that is the whole
        // premise of scrolling.
        inner.style.flex_shrink = 0.0;
        // Leave the overlay scrollbar its own lane. `inner` is in flow, so
        // padding narrows its stretched children — no plane needed here.
        if max_y > 0.5 {
            inner.style.padding.right = Dim::px(GUTTER as f32);
        }

        let mut el = Element {
            role: Role::ScrollArea,
            clip: true, // overflow:hidden — content outside the viewport is masked

            actions: vec![Action::ScrollIntoView],
            style: LayoutStyle {
                // Anchors the absolutely-positioned overlay scrollbar.
                position: Position::Relative,
                height: Dim::px(viewport_h as f32),
                ..LayoutStyle::default()
            },
            // Positive wheel delta scrolls toward the end (the shell normalizes
            // the OS sign so wheel-down moves the content down).

            // W3: keyboard scrolling. Focusable so Tab can reach the viewport —
            // a scroll region a keyboard user cannot move is a real a11y gap
            // (WAI-ARIA: a scrollable region must be keyboard operable).
            focusable: true,

            children: match overlay_scrollbar(&name, viewport_h, y, max_y, offset) {
                Some(bar) => vec![inner, bar],
                None => vec![inner],
            },
            ..Element::default()
        }
        .set_scroll(Some(ScrollInfo {
            x: 0.0,
            y,
            max_x: 0.0,
            max_y,
        }))
        .set_on_key(Some(Rc::new(move |rt, ke| {
            let line = lumen_core::events::WHEEL_LINE_PX;
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
        })))
        .set_on_wheel(Some(Rc::new(move |rt, _dx, dy, _mods| {
            offset.update(rt, |o| *o = (*o + dy).clamp(0.0, max_y))
        })));
        common.apply(&mut el);
        el
    }
}

impl_widget!(Scrollable);

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
/// Width of the overlay scrollbar's track.
pub(crate) const TRACK_W: f64 = 8.0;
/// Horizontal space scroll content leaves free on its right so rows stop short
/// of the bar instead of running under it. The bar is an *overlay* — it takes
/// no layout space of its own — so without this every scrolling widget paints
/// its content beneath the thumb.
pub(crate) const GUTTER: f64 = TRACK_W + 4.0;

/// Hold absolutely-positioned scroll content clear of the overlay scrollbar.
///
/// The rows are `position: absolute` with `width: 100%`, and a percentage
/// resolves against the containing block's *padding* box — so padding on the
/// viewport cannot narrow them. An intermediate plane pinned `left: 0,
/// right: GUTTER` has a definite width that excludes the bar, and the rows
/// resolve against that instead. `elide_semantics` keeps it out of the
/// semantic tree, so selectors and tests see the same shape as before.
pub(crate) fn gutter_plane(children: Vec<Element>) -> Element {
    Element {
        role: lumen_core::semantics::Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                right: Dim::px(GUTTER as f32),
                top: Dim::px(0.0),
                bottom: Dim::px(0.0),
            },
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

/// A stable id for the scroll surface named `name`, safe to use as a selector.
///
/// Ids are `[a-z0-9-]` (a dot would parse as id+class and be unselectable), and
/// scroll-state names are author-chosen — `grid.sheet` is a legal signal key —
/// so anything else folds to a dash.
fn bar_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("{slug}-scrollbar")
}

pub(crate) fn overlay_scrollbar(
    name: &str,
    viewport_h: f64,
    y: f64,
    max_y: f64,
    offset: lumen_core::state::Signal<f64>,
) -> Option<Element> {
    if max_y <= 0.5 || viewport_h <= 0.0 {
        return None;
    }
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

    Some(
        Element {
            // No `ScrollBar` role exists; `Slider` is the closest — ARIA models a
            // scrollbar as a range widget — and it keeps the bar drivable by the
            // agent rather than invisible to it. A dedicated role would be more
            // precise if this proves confusing to a screen reader.
            role: Role::Slider,
            label: "Scrollbar".to_string(),
            // A drag is re-resolved by stable id across rebuilds, falling back to
            // the raw node index only when there is no id — and scrolling rebuilds
            // on every frame of the drag. The bar was the one draggable control
            // with no id, so its grab rode on an index that the rebuild was free to
            // renumber. It is also the only way an agent or a test can address the
            // bar at all.
            id: Some(bar_id(name).into()),
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

            children: vec![thumb],
            ..Element::default()
        }
        .set_on_drag(Some(Rc::new(move |rt, _fx, fy, _pos| {
            offset.set(rt, (fy * max_y).clamp(0.0, max_y));
        }))),
    )
}
