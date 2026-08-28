//! [`Accordion`] — a self-stateful disclosure (show/hide) section. Its `Element`
//! (a clickable header plus, *only when open*, the caller's body) is built inside
//! [`Accordion::new`] / [`Accordion::body`]; the open/closed flag lives in a
//! boolean signal keyed by `name`.

use crate::widget::{impl_widget, Common, Widget};
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
    name: String,
    title: crate::Text,
    /// The disclosure flag, read where the `BuildCx` is.
    is_open: bool,
    open: lumen_core::state::Signal<bool>,
    /// The body, collected only while the section is open.
    body: Option<Vec<Element>>,
    color: Option<Color>,
    common: Common,
}

impl Accordion {
    /// A disclosure section titled `title`, open/closed under `name`.
    pub fn new(cx: &BuildCx, name: &str, title: impl Into<crate::Text>) -> Accordion {
        let open = cx.signal(name, || false);
        Accordion {
            name: name.to_string(),
            title: title.into(),
            is_open: open.get(cx.runtime()),
            open,
            body: None,
            color: None,
            common: Common::default(),
        }
    }

    /// Mount `content` inside the section (shown only while it is open).
    pub fn body(mut self, content: impl IntoIterator<Item = Element>) -> Accordion {
        // Collected only when it will be shown, exactly as before — a collapsed
        // section pays nothing for a body it will not mount.
        if self.is_open {
            self.body = Some(content.into_iter().collect());
        }
        self
    }

    /// Whether the section named `name` is currently open.
    pub fn is_open(cx: &BuildCx, name: &str) -> bool {
        cx.signal(name, || false).get(cx.runtime())
    }

    /// Set the title's text colour.
    pub fn color(mut self, c: Color) -> Accordion {
        self.color = Some(c);
        self
    }
}

impl Accordion {
    /// This node and its children, never joined — see `Container::parts`.
    fn parts(self) -> (Element, Vec<Element>) {
        let Accordion {
            name,
            title,
            is_open,
            open,
            body,
            color,
            common,
        } = self;

        // Header: chevron + title in a row. Clicking it (or Space/Enter when
        // focused, which routes to `on_click`) flips the flag.
        let chevron = Element::text(if is_open { "▼" } else { "▶" });
        let (title_s, title_dyn) = title.clone().into_parts();
        // The colour is applied to the title as it is made, rather than walked
        // back to through `children[0].children.last()`.
        let mut title_el = Element::text(title);
        if let Some(c) = color {
            if let Some(ts) = title_el.text_style_mut() {
                ts.color = c;
            }
        }
        let header = Element {
            role: Role::Button,
            label: title_s,
            dyn_text: title_dyn,
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
            children: vec![chevron, title_el],
            ..Element::default()
        };

        let mut children = vec![header];
        if let Some(content) = body {
            children.push(Element {
                id: Some(format!("{name}-body").into()),
                role: Role::Group,
                style: LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    row_gap: Dim::px(6.0),
                    ..LayoutStyle::default()
                },
                children: content,
                ..Element::default()
            });
        }

        // The outer node mirrors the disclosure state in semantics so the agent
        // sees expanded/collapsed on the section as a whole.
        let mut el = Element {
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
            ..Element::default()
        };
        common.apply(&mut el);
        (el, children)
    }
}

impl Widget for Accordion {
    fn build(self) -> Element {
        let (mut el, children) = self.parts();
        el.children = children;
        el
    }
}

impl crate::Direct for Accordion {
    fn lower_owned(
        self,
        w: &mut dyn crate::NodeWriter,
        parent: Option<crate::NodeIndex>,
        in_overlay: bool,
    ) -> (crate::NodeIndex, crate::LayoutNode) {
        let (el, children) = self.parts();
        w.write_children(el, children, parent, in_overlay)
    }
}

impl_widget!(Accordion, native);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use lumen_core::events::{Event, PointerEvent};
    use lumen_core::geometry::{Point, Size};
    use lumen_core::state::Signal;

    /// Press *and release* at a window point. Both halves are required: the
    /// click fires on the release, so a lone `PointerDown` activates nothing.
    fn click(h: &mut crate::Headless, x: f64, y: f64) {
        let p = Point::new(x, y);
        h.inject(Event::PointerDown(PointerEvent::at(p)));
        h.inject(Event::PointerUp(PointerEvent::at(p)));
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
        click(&mut h, 10.0, 10.0);
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
        click(&mut h, 10.0, 10.0);
        h.pump();
        let open: Signal<bool> = h.runtime().signal("acc", || false);
        assert!(open.get(h.runtime()));
    }
}
