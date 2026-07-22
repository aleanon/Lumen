//! [`Sheet`] and [`Drawer`] (W.1) — modal panels sliding in from a window
//! edge: a full-window scrim that light-dismisses, plus a content panel
//! anchored to the bottom (`Sheet`) or a side (`Drawer`). The open flag
//! lives in a signal keyed by `name` (`{name}.open`), so any handler can
//! open one: `cx.signal("cart.open", || false).set(rt, true)`.

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};
use std::rc::Rc;

/// Which edge a [`Drawer`] slides from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DrawerSide {
    /// Left edge (default).
    #[default]
    Left,
    /// Right edge.
    Right,
}

fn scrim_and_panel(
    cx: &BuildCx,
    name: &str,
    content: Element,
    panel_style: LayoutStyle,
) -> Element {
    let open = cx.signal(&format!("{name}.open"), || false);
    if !open.get(cx.runtime()) {
        // Closed: a zero-size placeholder keeps the widget's identity stable
        // without occupying layout.
        let mut empty = Element::default();
        empty.style.display = lumen_layout::Display::None;
        return empty;
    }

    let mut scrim = Element {
        role: Role::Generic,
        background: Some(Color::srgb8(0x10, 0x14, 0x18, 0x66)),
        overlay: true,
        ..Element::default()
    };
    scrim.style.position = Position::Absolute;
    scrim.style.inset = Edges::all(Dim::px(0.0));
    scrim.on_dismiss = Some(Rc::new(move |rt| open.set(rt, false)));
    scrim.on_click = Some(Rc::new(move |rt| open.set(rt, false)));

    let panel = Element {
        role: Role::Dialog,
        background: Some(Color::srgb8(0xff, 0xff, 0xff, 0xff)),
        corner_radius: 12.0,
        shadow: Some(crate::element::Shadow::soft()),
        overlay: true,
        style: panel_style,
        children: vec![content],
        ..Element::default()
    };

    let mut wrap = Element {
        role: Role::Group,
        children: vec![scrim, panel],
        ..Element::default()
    };
    // The wrapper is a full-window layer. Sized explicitly from the build's
    // window size (a resize rebuilds): the root element is content-sized, so
    // `inset: 0` alone would collapse to the content box, not the window.
    wrap.style.position = Position::Absolute;
    wrap.style.inset = Edges {
        left: Dim::px(0.0),
        top: Dim::px(0.0),
        ..Edges::AUTO
    };
    let win = cx.size();
    wrap.style.width = Dim::px(win.width as f32);
    wrap.style.height = Dim::px(win.height as f32);
    wrap
}

/// A modal bottom sheet.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{widgets, Sheet, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // The sheet fills the window (scrim + panel), so no demo wrapper is needed.
///     Sheet::new(cx, "sheet", widgets::text("Sheet content")).into()
/// }
/// # let app = App::new(build);
/// # // The panel is hidden until `sheet.open` is set (see the module docs).
/// # lumen_widgets::doc_shot_open(app, 240.0, 160.0, "sheet", "sheet.open");
/// ```
///
/// Renders:
///
/// ![Sheet example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/sheet.png)
///
/// The picture above is `src/doc_shots/sheet.png` — this exact example's
/// output. `doc_shot_open` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Sheet {
    el: Element,
}

impl Sheet {
    /// A bottom sheet whose open flag is `{name}.open`; `content` fills the
    /// panel. Scrim click / Escape closes it.
    pub fn new(cx: &BuildCx, name: &str, content: Element) -> Sheet {
        let style = LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                right: Dim::px(0.0),
                bottom: Dim::px(0.0),
                ..Edges::AUTO
            },
            padding: Edges::all(Dim::px(16.0)),
            ..LayoutStyle::default()
        };
        Sheet {
            el: scrim_and_panel(cx, name, content, style),
        }
    }
}

impl_common!(Sheet);

/// A modal side drawer.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{widgets, Drawer, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     Drawer::new(cx, "drawer", widgets::text("Menu")).into()
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot_open(app, 280.0, 160.0, "drawer", "drawer.open");
/// ```
///
/// Renders:
///
/// ![Drawer example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/drawer.png)
///
/// The picture above is `src/doc_shots/drawer.png` — this exact example's
/// output. `doc_shot_open` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Drawer {
    el: Element,
    side: DrawerSide,
}

impl Drawer {
    /// A drawer (default: from the left, 300 px wide) whose open flag is
    /// `{name}.open`.
    pub fn new(cx: &BuildCx, name: &str, content: Element) -> Drawer {
        let style = LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                top: Dim::px(0.0),
                bottom: Dim::px(0.0),
                ..Edges::AUTO
            },
            width: Dim::px(300.0),
            padding: Edges::all(Dim::px(16.0)),
            ..LayoutStyle::default()
        };
        Drawer {
            el: scrim_and_panel(cx, name, content, style),
            side: DrawerSide::Left,
        }
    }

    /// Slide from the right edge instead.
    pub fn side(mut self, side: DrawerSide) -> Self {
        self.side = side;
        if side == DrawerSide::Right {
            if let Some(panel) = self.el.children.get_mut(1) {
                panel.style.inset = Edges {
                    right: Dim::px(0.0),
                    top: Dim::px(0.0),
                    bottom: Dim::px(0.0),
                    ..Edges::AUTO
                };
            }
        }
        self
    }

    /// The configured side (introspection/tests).
    pub fn current_side(&self) -> DrawerSide {
        self.side
    }
}

impl_common!(Drawer);
