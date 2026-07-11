//! [`Popover`] — an anchored floating panel (W.1): the generalization of
//! `PickList`'s trigger + overlay pattern for arbitrary content. The open
//! flag lives in a signal keyed by `name`; the panel paints above siblings
//! (overlay), escapes ancestor clips, and light-dismisses (click-away /
//! Escape).

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use std::rc::Rc;

/// Which side of the trigger the panel opens on. Screen-edge auto-flipping
/// is deferred: placement happens at build time, before layout, so the
/// widget cannot see its own position yet (the same pre-layout constraint
/// container queries solve with a bounded re-pass — a candidate follow-up).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PopoverSide {
    /// Panel below the trigger (default).
    #[default]
    Below,
    /// Panel above the trigger.
    Above,
}

/// An anchored floating panel: a trigger element that toggles an overlay.
pub struct Popover {
    el: Element,
}

impl Popover {
    /// A popover whose open flag is stored under `{name}.open`. `trigger`
    /// toggles it; `content` renders inside the floating panel while open.
    pub fn new(cx: &BuildCx, name: &str, mut trigger: Element, content: Element) -> Popover {
        let open = cx.signal(&format!("{name}.open"), || false);
        let is_open = open.get(cx.runtime());

        if trigger.role == Role::Generic {
            trigger.role = Role::Button;
        }
        trigger.focusable = true;
        let prev = trigger.on_click.take();
        trigger.on_click = Some(Rc::new(move |rt| {
            if let Some(p) = &prev {
                p(rt);
            }
            open.update(rt, |o| *o = !*o);
        }));

        let mut children = vec![trigger];
        if is_open {
            let mut panel = Element {
                role: Role::Dialog,
                background: Some(Color::srgb8(0xff, 0xff, 0xff, 0xff)),
                corner_radius: 8.0,
                shadow: Some(crate::element::Shadow::soft()),
                overlay: true,
                children: vec![content],
                ..Element::default()
            };
            panel.on_dismiss = Some(Rc::new(move |rt| open.set(rt, false)));
            panel.style.position = Position::Absolute;
            panel.style.padding = Edges::all(Dim::px(10.0));
            // Anchored just below the trigger: the wrapper is exactly the
            // trigger's box (the panel is absolute), so `top: 100%` lands at
            // its bottom edge; a small margin adds the gap.
            panel.style.inset = Edges {
                left: Dim::px(0.0),
                top: Dim::pct(1.0),
                ..Edges::AUTO
            };
            panel.style.margin.top = Dim::px(4.0);
            children.push(panel);
        }

        let el = Element {
            role: Role::Group,
            style: LayoutStyle {
                position: Position::Relative,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        Popover { el }
    }

    /// Open the panel above instead of below the trigger.
    pub fn side(mut self, side: PopoverSide) -> Self {
        if side == PopoverSide::Above {
            if let Some(panel) = self.el.children.get_mut(1) {
                panel.style.inset = Edges {
                    left: Dim::px(0.0),
                    bottom: Dim::pct(1.0),
                    ..Edges::AUTO
                };
                panel.style.margin.top = Dim::px(0.0);
                panel.style.margin.bottom = Dim::px(4.0);
            }
        }
        self
    }
}

impl_common!(Popover);
