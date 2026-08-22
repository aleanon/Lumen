//! Widgets that float above the page: dropdowns, tooltips, menus and modal
//! dialogs. What they share is an overlay pass and a dismiss contract.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::impl_common;
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::{Color, Runtime};
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// [`Select`] — a combo box cycling through `options` on click; selected
/// index under `name` (typed form of [`select`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Select, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Select::new(cx, "sel", &["Red", "Green", "Blue"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 160.0, 56.0, "select");
/// ```
///
/// Renders:
///
/// ![Select example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/select.png)
///
/// The picture above is `src/doc_shots/select.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Select {
    el: Element,
}

impl Select {
    /// A select / combo box cycling through `options` on click. `name` keys the
    /// selected index; the semantic value is the current option.
    pub fn new(cx: &BuildCx, name: &str, options: &[&str]) -> Select {
        let el = {
            let idx = cx.signal(name, || 0usize);
            let i = idx.get(cx.runtime()).min(options.len().saturating_sub(1));
            let cur = options.get(i).copied().unwrap_or_default().to_string();
            let n = options.len();
            Element {
                role: Role::ComboBox,
                label: cur.clone(),
                value: Some(cur.clone()),
                focusable: true,
                actions: vec![Action::Click, Action::Focus, Action::SetValue],
                background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
                corner_radius: 4.0,
                style: LayoutStyle {
                    padding: Edges::all(Dim::px(6.0)),
                    min_width: Dim::px(120.0),
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(format!("{cur} ▾"), TextStyle::default()),
                on_click: Some(Rc::new(move |rt| {
                    idx.update(rt, |x| *x = (*x + 1) % n.max(1))
                })),
                ..Element::default()
            }
        };
        Select { el }
    }
}

impl_common!(Select);

/// A select / combo box cycling through `options` on click. `name` keys the
/// selected index; the semantic value is the current option.
/// *(Thin shim over [`Select`] — the typed form is preferred.)*
pub fn select(cx: &BuildCx, name: &str, options: &[&str]) -> Element {
    Select::new(cx, name, options).into()
}

/// [`Tooltip`] — wraps `target` with hover-revealed help `text` (typed
/// form of [`tooltip`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, Tooltip, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     // The tip is transient: it appears while `#tip-tip-host` is hovered, so
///     // the doc render shows the resting state.
///     centered(cx, Tooltip::new(cx, "tip", widgets::text("hover me"), "A helpful hint").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 180.0, 64.0, "tooltip");
/// ```
///
/// Renders:
///
/// ![Tooltip example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/tooltip.png)
///
/// The picture above is `src/doc_shots/tooltip.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Tooltip {
    el: Element,
}

impl Tooltip {
    /// Wrap `target` with a tooltip that appears **while the target is
    /// hovered**, floating above the content.
    ///
    /// The previous implementation stacked the tip permanently in a column
    /// under the target: always visible, and it displaced whatever came after
    /// it. A tooltip is transient and must not affect layout, so the tip is an
    /// absolutely-positioned overlay rendered only on hover — matching every
    /// other toolkit, and leaving `target`'s own box untouched.
    ///
    /// `name` keys the hover test, so the target gets a stable id.
    pub fn new(cx: &BuildCx, name: &str, target: Element, text: impl Into<crate::Text>) -> Tooltip {
        let text = text.into();
        let host_id = format!("{name}-tip-host");
        let showing = cx.is_hovered(&host_id);

        let mut host = target;
        host.id = Some(host_id.clone().into());
        // The tip describes the target, so the target carries the description
        // for assistive tech whether or not the tip is currently painted.
        let (text_s, text_dyn) = text.clone().into_parts();
        if host.label.is_empty() {
            host.label = text_s.clone();
        }

        let mut children = vec![host];
        if showing {
            let tip = Element {
                role: Role::Tooltip,
                label: text_s.clone(),
                dyn_text: text_dyn,
                overlay: true,
                background: Some(Color::srgb8(0x20, 0x24, 0x2a, 0xff)),
                corner_radius: 6.0,
                style: LayoutStyle {
                    position: lumen_layout::Position::Absolute,
                    inset: Edges {
                        top: Dim::px(-28.0),
                        left: Dim::px(0.0),
                        ..Edges::AUTO
                    },
                    padding: Edges {
                        left: Dim::px(8.0),
                        right: Dim::px(8.0),
                        top: Dim::px(4.0),
                        bottom: Dim::px(4.0),
                    },
                    ..LayoutStyle::default()
                },
                content: crate::NodeContent::Text(
                    text_s,
                    TextStyle {
                        font_size: 12.0,
                        weight: 400.0,
                        color: Color::srgb8(0xff, 0xff, 0xff, 0xff),
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                        features: None,
                        variations: None,
                        italic: false,
                        align: Default::default(),
                    },
                ),
                ..Element::default()
            };
            children.push(tip);
        }

        let el = Element {
            role: Role::Group,
            elide_semantics: true,
            style: LayoutStyle {
                position: lumen_layout::Position::Relative,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        Tooltip { el }
    }
}

impl_common!(Tooltip);

/// Wrap `target` with a tooltip whose `text` is exposed to assistive tech.
/// *(Thin shim over [`Tooltip`] — the typed form is preferred.)*
pub fn tooltip(cx: &BuildCx, name: &str, target: Element, text: impl Into<crate::Text>) -> Element {
    Tooltip::new(cx, name, target, text).into()
}

/// [`Menu`] — a vertical list of menu items (typed form of [`menu`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Menu, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Menu::new(&["New", "Open", "Save", "Quit"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 160.0, 170.0, "menu");
/// ```
///
/// Renders:
///
/// ![Menu example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/menu.png)
///
/// The picture above is `src/doc_shots/menu.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Menu {
    el: Element,
    items: Vec<String>,
}

/// Shared select handler for [`Menu`] items: `(runtime, item index)`.
type SelectHandler = Rc<dyn Fn(&Runtime, usize)>;

impl Menu {
    /// A vertical menu of selectable items.
    ///
    /// Attach [`Menu::on_select`] to react to a choice — without it the items
    /// are inert. (They still carry `Role::MenuItem` + a label, so the menu
    /// reads correctly, but `actions` stays empty rather than advertising a
    /// `Click` nothing implements.)
    pub fn new(items: &[&str]) -> Menu {
        let items: Vec<String> = items.iter().map(|s| (*s).to_string()).collect();
        let el = Element {
            role: Role::Menu,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children: items.iter().map(|t| Self::item(t, None)).collect(),
            ..Element::default()
        };
        Menu { el, items }
    }

    /// One menu item. `on` is the shared select handler; with `None` the item
    /// declares no actions (W2: never advertise what isn't implemented).
    fn item(text: &str, on: Option<(SelectHandler, usize)>) -> Element {
        let mut el = Element {
            role: Role::MenuItem,
            label: text.to_string(),
            focusable: true,
            style: LayoutStyle {
                padding: Edges::all(Dim::px(6.0)),
                ..LayoutStyle::default()
            },
            content: crate::NodeContent::Text(text.to_string(), TextStyle::default()),
            ..Element::default()
        };
        el.actions = vec![Action::Focus];
        if let Some((f, i)) = on {
            el.actions.push(Action::Click);
            el.on_click = Some(Rc::new(move |rt| f(rt, i)));
        }
        el
    }

    /// Run `f(rt, index)` when an item is chosen (click, or Enter/Space while
    /// focused — the framework routes focused activation to `on_click`).
    pub fn on_select(mut self, f: impl Fn(&Runtime, usize) + 'static) -> Menu {
        let f: SelectHandler = Rc::new(f);
        self.el.children = self
            .items
            .iter()
            .enumerate()
            .map(|(i, t)| Self::item(t, Some((Rc::clone(&f), i))))
            .collect();
        self
    }
}

impl_common!(Menu);

/// A vertical menu of selectable items; `on_select` receives the item index.
///
/// *(Thin shim over [`Menu`] — the typed form is preferred.)*
///
/// SD3: this previously took only `items` and could therefore never reach
/// [`Menu::on_select`], so every menu built through it was inert — it rendered,
/// it was clickable, and nothing happened. Selecting is the entire purpose of a
/// menu, so the handler is a parameter rather than an optional builder step.
pub fn menu(items: &[&str], on_select: impl Fn(&Runtime, usize) + 'static) -> Element {
    Menu::new(items).on_select(on_select).into()
}

/// [`Modal`] — `base` content with an optional centered `dialog` overlay
/// when `open` (typed form of [`modal`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{widgets, Container, Modal, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_layout::Dim;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let dialog = Container::new(vec![widgets::text("Dialog")])
///         .padding(16.0)
///         .background(Color::WHITE);
///     let mut modal: Element =
///         Modal::new(widgets::text("Page behind"), dialog.into(), true).into();
///     // The modal stacks a full-bleed backdrop over the page, so size it to the
///     // window — then the dialog centers over the whole frame.
///     let win = cx.size();
///     modal.style.width = Dim::px(win.width as f32);
///     modal.style.height = Dim::px(win.height as f32);
///     modal
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 160.0, "modal");
/// ```
///
/// Renders:
///
/// ![Modal example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/modal.png)
///
/// The picture above is `src/doc_shots/modal.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Modal {
    el: Element,
}

impl Modal {
    /// A modal overlay (E8.2): when `open`, `dialog` is shown centered over `base`
    /// with a dimmed backdrop; otherwise just `base`.
    pub fn new(base: Element, dialog: Element, open: bool) -> Modal {
        let el = {
            if !open {
                base
            } else {
                let backdrop = Element {
                    role: Role::Group,
                    background: Some(Color::srgb8(0x00, 0x00, 0x00, 0x88)),
                    style: LayoutStyle {
                        position: lumen_layout::Position::Absolute,
                        inset: Edges::all(Dim::px(0.0)),
                        display: Display::Flex,
                        align_items: Some(Align::Center),
                        justify_content: Some(Align::Center),
                        width: Dim::pct(1.0),
                        height: Dim::pct(1.0),
                        ..LayoutStyle::default()
                    },
                    children: vec![dialog],
                    ..Element::default()
                }
                .id("modal-overlay");
                crate::widgets::stack(vec![base, backdrop])
            }
        };
        Modal { el }
    }
}

impl_common!(Modal);

/// A modal overlay (E8.2): when `open`, `dialog` is shown centered over `base`
/// with a dimmed backdrop; otherwise just `base`.
/// *(Thin shim over [`Modal`] — the typed form is preferred.)*
pub fn modal(base: Element, dialog: Element, open: bool) -> Element {
    Modal::new(base, dialog, open).into()
}
