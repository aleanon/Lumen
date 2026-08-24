//! [`Card`] and [`Badge`] (W5) — the two containers a real screen reaches for
//! first and Lumen had no answer for.
//!
//! `Card` is Material's surface: a padded, rounded, elevated box grouping
//! related content. `Badge` is the small count/dot overlaid on another widget
//! (unread messages, cart items).

use crate::widget::{impl_widget, Common, Widget};
use crate::{widgets, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

/// A surface grouping related content: padded, rounded, and raised off the
/// background.
///
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Card, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let body = vec![
///         widgets::text("Total balance"),
///         widgets::text("$1,240.00"),
///     ];
///     centered(cx, Card::new(body).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 260.0, 140.0, "card");
/// ```
///
/// Renders:
///
/// ![Card example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/card.png)
///
/// The picture above is `src/doc_shots/card.png` — this exact example's output.
/// `doc_shot` re-renders it every test run and fails if the render drifts from
/// that committed image, so the picture is always current.
pub struct Card {
    children: Vec<Element>,
    /// A heading, rendered above the content and reused as the group's
    /// accessible label.
    title: Option<crate::Text>,
    /// Set when the whole card is activatable (a tappable list card).
    on_press: Option<crate::Handler>,
    /// Flat: keep the padding and radius, drop the shadow.
    flat: bool,
    common: Common,
}

impl Card {
    /// A card wrapping `children` in a column.
    pub fn new(children: impl Into<Vec<Element>>) -> Card {
        Card {
            children: children.into(),
            title: None,
            on_press: None,
            flat: false,
            common: Common::default(),
        }
    }

    /// Give the card a heading, rendered above its content and used as the
    /// group's accessible label so the card announces as a unit.
    pub fn title(mut self, title: impl Into<crate::Text>) -> Card {
        self.title = Some(title.into());
        self
    }

    /// Make the whole card activatable (a tappable list card).
    pub fn on_press(mut self, f: impl Fn(&lumen_core::state::Runtime) + 'static) -> Card {
        self.on_press = Some(Rc::new(f));
        self
    }

    /// Flatten the card: keep the padding and radius, drop the shadow.
    pub fn flat(mut self) -> Card {
        self.flat = true;
        self
    }
}

impl Widget for Card {
    fn build(self) -> Element {
        let Card {
            mut children,
            title,
            on_press,
            flat,
            common,
        } = self;

        // The heading is prepended here rather than in `.title()`, so it stays
        // first even when `.children()`-style edits follow it in the chain.
        let mut label = String::new();
        if let Some(title) = title {
            label = title.as_static().unwrap_or_default().to_string();
            let mut t = widgets::text(title);
            if let Some(ts) = t.text_style_mut() {
                ts.font_size = 15.0;
                ts.weight = 600.0;
            }
            children.insert(0, t.class("card-title"));
        }

        let activatable = on_press.is_some();
        let mut el = Element {
            role: if activatable { Role::Button } else { Role::Group },
            label,
            focusable: activatable,
            actions: if activatable {
                vec![Action::Click, Action::Focus]
            } else {
                Vec::new()
            },
            on_click: on_press,
            background: Some(Color::srgb8(0xff, 0xff, 0xff, 0xff)),
            corner_radius: 12.0,
            shadow: if flat {
                None
            } else {
                Some(crate::element::Shadow::soft())
            },
            classes: vec!["card".to_string()],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Dim::px(8.0),
                padding: Edges::all(Dim::px(16.0)),
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Card);

/// A small count or dot overlaid on the top-right of another widget.
///
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Badge, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Badge::new(widgets::text("Inbox"), "3").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 180.0, 80.0, "badge");
/// ```
///
/// Renders:
///
/// ![Badge example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/badge.png)
///
/// The picture above is `src/doc_shots/badge.png` — this exact example's output.
/// `doc_shot` re-renders it every test run and fails if the render drifts from
/// that committed image, so the picture is always current.
pub struct Badge {
    target: Element,
    label: crate::Text,
    /// Pill colour override.
    color: Option<Color>,
    /// A bare dot with no count (Material's "small" badge).
    dot: bool,
    common: Common,
}

impl Badge {
    /// Overlay `label` on `target`'s top-right corner.
    ///
    /// The badge is absolutely positioned, so adding one never changes the
    /// target's layout. Its text is announced as part of the group, so a screen
    /// reader hears "Inbox 3" rather than losing the count.
    pub fn new(target: Element, label: impl Into<crate::Text>) -> Badge {
        Badge {
            target,
            label: label.into(),
            color: None,
            dot: false,
            common: Common::default(),
        }
    }

    /// Recolour the badge pill.
    pub fn color(mut self, c: Color) -> Badge {
        self.color = Some(c);
        self
    }

    /// A bare dot with no count (Material's "small" badge).
    ///
    /// A flag rather than a walk into `children[1]` that cleared the count
    /// element `::new()` had just built and styled.
    pub fn dot(mut self) -> Badge {
        self.dot = true;
        self
    }
}

impl Widget for Badge {
    fn build(self) -> Element {
        let Badge {
            target,
            label,
            color,
            dot,
            common,
        } = self;
        let (label_s, label_dyn) = label.clone().into_parts();

        // A dot carries no count, so the run is never made in the first place.
        let children = if dot {
            Vec::new()
        } else {
            let mut t = widgets::text(label);
            if let Some(ts) = t.text_style_mut() {
                ts.font_size = 11.0;
                ts.weight = 600.0;
                ts.color = Color::WHITE;
            }
            vec![t]
        };

        let pill = Element {
            role: Role::Text,
            label: if dot { "unread".to_string() } else { label_s },
            dyn_text: if dot { None } else { label_dyn },
            background: Some(color.unwrap_or(Color::srgb8(0xd3, 0x2f, 0x2f, 0xff))),
            corner_radius: 999.0,
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    top: Dim::px(-9.0),
                    right: Dim::px(-14.0),
                    ..Edges::AUTO
                },
                display: Display::Flex,
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                min_width: Dim::px(if dot { 10.0 } else { 18.0 }),
                height: Dim::px(if dot { 10.0 } else { 18.0 }),
                padding: if dot {
                    Edges::all(Dim::px(0.0))
                } else {
                    Edges {
                        left: Dim::px(5.0),
                        right: Dim::px(5.0),
                        top: Dim::px(0.0),
                        bottom: Dim::px(0.0),
                    }
                },
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        }
        .class("badge");

        let mut el = Element {
            role: Role::Group,
            style: LayoutStyle {
                position: Position::Relative,
                display: Display::Flex,
                ..LayoutStyle::default()
            },
            children: vec![target, pill],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Badge);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{App, BuildCx};
    use kurbo::Size;
    use lumen_core::semantics::SemanticsNode;

    fn find(n: &SemanticsNode, id: &str) -> Option<SemanticsNode> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.clone());
        }
        n.children.iter().find_map(|c| find(c, id))
    }

    #[test]
    fn a_card_groups_and_labels_its_content() {
        let mut h = App::new(|_cx: &mut BuildCx| {
            Card::new(vec![widgets::text("body").id("body")])
                .title("Balance")
                .id("c")
                .into()
        })
        .run_headless(Size::new(300.0, 200.0));
        h.pump();

        let sem = h.semantics_doc().root.elided();
        let card = find(&sem, "c").expect("card in the tree");
        assert_eq!(card.label, "Balance", "the title labels the group");
        assert!(find(&sem, "body").is_some(), "content is inside it");
        h.assert_view_coherent();
    }

    #[test]
    fn a_pressable_card_is_a_button() {
        let mut h = App::new(|cx: &mut BuildCx| {
            let hits = cx.signal("hits", || 0i64);
            Card::new(vec![widgets::text("tap me")])
                .on_press(move |rt| hits.update(rt, |n| *n += 1))
                .id("c")
                .into()
        })
        .run_headless(Size::new(300.0, 200.0));
        h.pump();

        h.invoke_action("#c", "click")
            .expect("a pressable card acts");
        let hits: lumen_core::Signal<i64> = h.runtime().signal("hits", || 0i64);
        assert_eq!(hits.get(h.runtime()), 1);
        // W2: it must not declare anything it cannot do.
        assert!(h.audit_actions().is_empty(), "{:?}", h.audit_actions());
    }

    /// A badge must not disturb what it decorates — that is why it is absolute.
    #[test]
    fn a_badge_does_not_move_its_target() {
        let plain = {
            let mut h = App::new(|_cx: &mut BuildCx| widgets::text("Inbox").id("t"))
                .run_headless(Size::new(200.0, 100.0));
            h.pump();
            h.node_bounds_by_id("t").expect("laid out")
        };
        let badged = {
            let mut h = App::new(|_cx: &mut BuildCx| {
                Badge::new(widgets::text("Inbox").id("t"), "3").into()
            })
            .run_headless(Size::new(200.0, 100.0));
            h.pump();
            h.node_bounds_by_id("t").expect("laid out")
        };
        assert_eq!(plain, badged, "adding a badge must not reflow the target");
    }
}
