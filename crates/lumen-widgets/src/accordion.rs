//! [`Accordion`] — a self-stateful disclosure (show/hide) section. Its `Element`
//! (a clickable header plus, *only when open*, the caller's body) is built inside
//! [`Accordion::new`] / [`Accordion::body`]; the open/closed flag lives in a
//! boolean signal keyed by `name`.

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use std::rc::Rc;

/// A disclosure section: a focusable header with a title and a chevron
/// (`▶` collapsed, `▼` expanded), plus a body that is present in the tree **only
/// when open**. Clicking the header (or Space/Enter while it is focused) toggles
/// the boolean stored under `name`. Supply the body with [`Accordion::body`].
///
/// The body is conditional *structure*, not a hidden style flag: when collapsed
/// the content nodes are absent from the element tree entirely, so they cost
/// nothing to lay out and are invisible to the agent / a11y tree.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, widgets, Accordion, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let acc = Accordion::new(cx, "faq", "What is Lumen?")
///         .body([widgets::text("An AI-first GUI framework.")]);
///     full_width(cx, acc.into())
/// }
/// # let app = App::new(build);
/// # // Rendered expanded (`faq`).
/// # lumen_widgets::doc_shot_open(app, 320.0, 120.0, "accordion", "faq");
/// ```
///
/// Renders:
///
/// ![Accordion example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/accordion.png)
///
/// The picture above is `src/doc_shots/accordion.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Accordion {
    el: Element,
    /// The state key, so [`Accordion::body`] can tag its panel `{name}-body`.
    name: String,
    /// Snapshot of the open-state signal at build time (a pure function of it), so
    /// [`Accordion::body`] mounts the content iff the section is open.
    is_open: bool,
}

impl Accordion {
    /// A collapsed-by-default disclosure titled `title`, its open/closed flag
    /// stored under `name`. Add content with [`Accordion::body`].
    pub fn new(cx: &BuildCx, name: &str, title: impl Into<String>) -> Accordion {
        let title = title.into();
        let open = cx.signal(name, || false);
        let is_open = open.get(cx.runtime());

        // Header: chevron + title in a row. Clicking it (or Space/Enter when
        // focused, which routes to `on_click`) flips the flag.
        let chevron = Element::text(if is_open { "▼" } else { "▶" });
        let header = Element {
            role: Role::Button,
            label: title.clone(),
            focusable: true,
            actions: vec![
                Action::Focus,
                if is_open {
                    Action::Collapse
                } else {
                    Action::Expand
                },
            ],
            states: vec![if is_open {
                SemState::Expanded
            } else {
                SemState::Collapsed
            }],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(Align::Center),
                column_gap: Dim::px(8.0),
                ..LayoutStyle::default()
            },
            // Capture only the `Copy` signal handle (ADR-013); mutate in place.
            on_click: Some(Rc::new(move |rt| open.update(rt, |o| *o = !*o))),
            children: vec![chevron, Element::text(title)],
            ..Element::default()
        };

        // The outer node mirrors the disclosure state in semantics so the agent
        // sees expanded/collapsed on the section as a whole.
        let el = Element {
            role: Role::Group,
            states: vec![if is_open {
                SemState::Expanded
            } else {
                SemState::Collapsed
            }],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Dim::px(6.0),
                ..LayoutStyle::default()
            },
            children: vec![header],
            ..Element::default()
        };
        Accordion {
            el,
            name: name.to_string(),
            is_open,
        }
    }

    /// Supply the body shown when the section is open. The `content` nodes are
    /// mounted **only when open** — collapsed, the body is absent from the tree
    /// (conditional structure, not a hidden flag). Wrapped in a `Column`-styled
    /// panel tagged `{name}-body` for hit-testing / tests. Calling this again
    /// replaces any previous body.
    pub fn body(mut self, content: impl IntoIterator<Item = Element>) -> Accordion {
        // Drop any previously-mounted body (index 0 is always the header).
        self.el.children.truncate(1);
        if self.is_open {
            let panel = Element {
                id: Some(format!("{}-body", self.name).into()),
                role: Role::Group,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    row_gap: Dim::px(6.0),
                    ..LayoutStyle::default()
                },
                children: content.into_iter().collect(),
                ..Element::default()
            };
            self.el.children.push(panel);
        }
        self
    }

    /// Whether the accordion named `name` is currently open (an external reader
    /// for the app, mirroring the signal without a rebuild).
    pub fn is_open(cx: &BuildCx, name: &str) -> bool {
        cx.signal(name, || false).get(cx.runtime())
    }

    /// Set the header title colour (e.g. to match a dark theme).
    pub fn color(mut self, c: Color) -> Accordion {
        if let Some(ts) = self
            .el
            .children
            .first_mut()
            .and_then(|h| h.children.last_mut())
            .and_then(|t| t.text_style_mut())
        {
            ts.color = c;
        }
        self
    }
}

impl_common!(Accordion);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use lumen_core::events::{Event, PointerEvent};
    use lumen_core::geometry::{Point, Size};
    use lumen_core::state::Signal;

    /// A pointer-down event at a window point.
    fn down(x: f64, y: f64) -> Event {
        Event::PointerDown(PointerEvent::at(Point::new(x, y)))
    }

    /// Build an accordion whose body is a single id-tagged node, so we can assert
    /// on its presence via bounds.
    fn app() -> crate::Headless {
        App::new(|cx| {
            Accordion::new(cx, "acc", "Details")
                .body(vec![Element::text("hidden content").id("body-line")])
                .into()
        })
        .run_headless(Size::new(300.0, 200.0))
    }

    /// Closed by default: the body node is absent from the tree; opening it via a
    /// header click flips the signal and mounts the body. The coherence oracle
    /// (incremental == rebuild-fresh) holds across the toggle.
    #[test]
    fn toggles_open_and_mounts_body() {
        let mut h = app();
        let closed = h.pump();
        // Collapsed: the tagged body node is not laid out (not in the tree).
        assert!(
            h.node_bounds_by_id("body-line").is_none(),
            "body must be absent while collapsed"
        );
        h.assert_view_coherent();

        let open: Signal<bool> = h.runtime().signal("acc", || false);
        assert!(!open.get(h.runtime()), "starts collapsed");

        // Click the header (top-left of the section).
        h.inject(down(10.0, 10.0));
        let opened = h.pump();

        assert!(open.get(h.runtime()), "header click opened the section");
        assert!(
            h.node_bounds_by_id("body-line").is_some(),
            "body must be present once open"
        );
        assert!(
            opened.node_count > closed.node_count,
            "opening adds the body subtree ({} -> {})",
            closed.node_count,
            opened.node_count
        );
        h.assert_view_coherent();
    }

    /// The static `is_open` reader tracks the signal.
    #[test]
    fn is_open_reader_tracks_state() {
        let mut h = app();
        h.pump();
        h.inject(down(10.0, 10.0));
        h.pump();
        let open: Signal<bool> = h.runtime().signal("acc", || false);
        assert!(open.get(h.runtime()));
    }
}
