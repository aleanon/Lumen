//! Widgets that float above the page: dropdowns, tooltips, menus and modal
//! dialogs. What they share is an overlay pass and a dismiss contract.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::{impl_widget, Common, Widget};
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
    options: Vec<String>,
    /// The clamped current index, read where the `BuildCx` is.
    i: usize,
    idx: lumen_core::state::Signal<usize>,
    common: Common,
}

impl Select {
    /// A cycling select over `options`; the index lives in the signal keyed by
    /// `name`.
    pub fn new(cx: &BuildCx, name: &str, options: &[&str]) -> Select {
        let idx = cx.signal(name, || 0usize);
        Select {
            i: idx.get(cx.runtime()).min(options.len().saturating_sub(1)),
            options: options.iter().map(|o| (*o).to_string()).collect(),
            idx,
            common: Common::default(),
        }
    }
}

impl Widget for Select {
    fn build(self) -> Element {
        let Select {
            options,
            i,
            idx,
            common,
        } = self;
        let cur = options.get(i).cloned().unwrap_or_default();
        let n = options.len();
        let mut el = Element {
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
            // W2: `SetValue` was advertised but unimplemented, so the agent
            // and assistive tech could only *cycle* the select one click at
            // a time. Setting the value by option text selects it directly.
            on_set_value: Some(Rc::new(move |rt: &Runtime, v: &str| {
                if let Some(k) = options.iter().position(|o| o == v) {
                    idx.set(rt, k);
                }
            })),
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Select);

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
    target: Element,
    text: crate::Text,
    host_id: String,
    /// Whether the host is hovered. `cx.is_hovered` is a *tracked* read, so it
    /// stays where the `BuildCx` is.
    showing: bool,
    common: Common,
}

impl Tooltip {
    /// Show `text` above `target` while the pointer is over it.
    pub fn new(cx: &BuildCx, name: &str, target: Element, text: impl Into<crate::Text>) -> Tooltip {
        let host_id = format!("{name}-tip-host");
        Tooltip {
            showing: cx.is_hovered(&host_id),
            target,
            text: text.into(),
            host_id,
            common: Common::default(),
        }
    }
}

impl Widget for Tooltip {
    fn build(self) -> Element {
        let Tooltip {
            target,
            text,
            host_id,
            showing,
            common,
        } = self;

        let mut host = target;
        host.id = Some(host_id.into());
        // The tip describes the target, so the target carries the description
        // for assistive tech whether or not the tip is currently painted.
        let (text_s, text_dyn) = text.into_parts();
        if host.label.is_empty() {
            host.label = text_s.clone();
        }

        let mut children = vec![host];
        if showing {
            children.push(Element {
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
            });
        }

        let mut el = Element {
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
        common.apply(&mut el);
        el
    }
}

impl_widget!(Tooltip);

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
/// use lumen_widgets::{top, Menu, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let menu = Menu::button(cx, "file", "File ▾", &["New", "Open", "Save", "Quit"], |_, _| {});
///     top(cx, menu.into())
/// }
/// # let app = App::new(build);
/// # // Rendered with the panel open (`file.open`).
/// # lumen_widgets::doc_shot_open(app, 180.0, 210.0, "menu", "file.open");
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
    items: Vec<String>,
    /// The choice handler.
    ///
    /// The eager `.on_select()` had to **rebuild every item** that `::new()`
    /// had just created, because each item's `on_click` closes over the
    /// handler and its own index. Storing it means the items are built once.
    on_select: Option<SelectHandler>,
    common: Common,
}

/// A menu's choice handler, given the chosen item's index.
type SelectHandler = Rc<dyn Fn(&Runtime, usize)>;

impl Menu {
    /// A vertical menu listing `items`.
    pub fn new(items: &[&str]) -> Menu {
        Menu {
            items: items.iter().map(|s| (*s).to_string()).collect(),
            on_select: None,
            common: Common::default(),
        }
    }

    /// A button that opens `items` in a popover, closing on choice.
    pub fn button(
        cx: &BuildCx,
        name: &str,
        label: impl Into<crate::Text>,
        items: &[&str],
        on_select: impl Fn(&Runtime, usize) + 'static,
    ) -> crate::Popover {
        let open = cx.signal(format!("{name}.open"), || false);
        let panel: Element = Menu::new(items)
            .on_select(move |rt, i| {
                on_select(rt, i);
                // A menu closes on choice — leaving it open is how a
                // permanently-open list behaves, not a menu.
                open.set(rt, false);
            })
            .id(format!("{name}-panel"))
            .into();
        let trigger: Element = crate::Button::new(label)
            .id(format!("{name}-trigger"))
            .into();
        crate::Popover::new(cx, name, trigger, panel).id(name)
    }

    /// One menu row.
    fn item(text: String, on: Option<(SelectHandler, usize)>) -> Element {
        let mut el = Element {
            role: Role::MenuItem,
            label: text.clone(),
            focusable: true,
            style: LayoutStyle {
                padding: Edges::all(Dim::px(6.0)),
                ..LayoutStyle::default()
            },
            content: crate::NodeContent::Text(text, TextStyle::default()),
            ..Element::default()
        };
        el.actions = vec![Action::Focus];
        if let Some((f, i)) = on {
            el.actions.push(Action::Click);
            el.on_click = Some(Rc::new(move |rt| f(rt, i)));
        }
        el
    }

    /// Run `f` with the chosen item's index.
    pub fn on_select(mut self, f: impl Fn(&Runtime, usize) + 'static) -> Menu {
        self.on_select = Some(Rc::new(f));
        self
    }
}

impl Widget for Menu {
    fn build(self) -> Element {
        let Menu {
            items,
            on_select,
            common,
        } = self;
        let children = items
            .into_iter()
            .enumerate()
            .map(|(i, t)| Self::item(t, on_select.as_ref().map(|f| (Rc::clone(f), i))))
            .collect();
        let mut el = Element {
            role: Role::Menu,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Menu);

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
    base: Element,
    dialog: Element,
    open: bool,
    common: Common,
}

impl Modal {
    /// `dialog` centred over `base` behind a scrim while `open`.
    pub fn new(base: Element, dialog: Element, open: bool) -> Modal {
        Modal {
            base,
            dialog,
            open,
            common: Common::default(),
        }
    }
}

impl Widget for Modal {
    fn build(self) -> Element {
        let Modal {
            base,
            dialog,
            open,
            common,
        } = self;
        let mut el = if !open {
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
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Modal);

/// A modal overlay (E8.2): when `open`, `dialog` is shown centered over `base`
/// with a dimmed backdrop; otherwise just `base`.
/// *(Thin shim over [`Modal`] — the typed form is preferred.)*
pub fn modal(base: Element, dialog: Element, open: bool) -> Element {
    Modal::new(base, dialog, open).into()
}
