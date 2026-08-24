//! [`Toast`], [`Spinner`], and [`Chip`] (W.1) — promoted from the
//! `examples/toast` and `examples/loading_spinners` prototypes into the
//! library. Colors are built in (per kind) so the widgets work with no
//! stylesheet; the classes stay on the elements for `.lss` overrides.

use crate::widget::{impl_common, impl_widget, Common, Widget};
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align, Dim, Edges};
use std::rc::Rc;

/// Toast severity — sets the accent bar + background tint and the class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    /// Neutral information.
    Info,
    /// Success confirmation.
    Success,
    /// Warning.
    Warn,
    /// Error / destructive outcome.
    Danger,
}

impl ToastKind {
    fn class(self) -> &'static str {
        match self {
            ToastKind::Info => "info",
            ToastKind::Success => "success",
            ToastKind::Warn => "warn",
            ToastKind::Danger => "danger",
        }
    }
    fn accent(self) -> Color {
        match self {
            ToastKind::Info => Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
            ToastKind::Success => Color::srgb8(0x18, 0x8a, 0x42, 0xff),
            ToastKind::Warn => Color::srgb8(0xc9, 0x8a, 0x0b, 0xff),
            ToastKind::Danger => Color::srgb8(0xd3, 0x2f, 0x2f, 0xff),
        }
    }
}

/// A transient notification card: accent bar + title + body. Presentation
/// only — stacking/auto-hide policy belongs to the app (drive it from a
/// signal + `wake_at`).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Toast, ToastKind, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Toast::new(ToastKind::Success, "Saved", "Changes stored").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 320.0, 92.0, "toast");
/// ```
///
/// Renders:
///
/// ![Toast example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/toast.png)
///
/// The picture above is `src/doc_shots/toast.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Toast {
    kind: ToastKind,
    title: crate::Text,
    body: crate::Text,
    /// An optional trailing action.
    action: Option<(String, Rc<dyn Fn(&lumen_core::state::Runtime)>)>,
    /// Set by `auto_dismiss` once the toast has outlived its window.
    expired: bool,
    common: Common,
}

impl Toast {
    /// A toast of `kind` with a heading and a body line.
    pub fn new(
        kind: ToastKind,
        title: impl Into<crate::Text>,
        body: impl Into<crate::Text>,
    ) -> Toast {
        Toast {
            kind,
            title: title.into(),
            body: body.into(),
            action: None,
            expired: false,
            common: Common::default(),
        }
    }

    /// Add a trailing action button.
    pub fn action(
        mut self,
        label: impl Into<String>,
        f: impl Fn(&lumen_core::state::Runtime) + 'static,
    ) -> Toast {
        self.action = Some((label.into(), Rc::new(f)));
        self
    }

    /// Vanish `ms` after the toast first appeared.
    ///
    /// Recording expiry as a flag rather than overwriting the built node means
    /// an expired toast never assembles its bar, heading, body and action at
    /// all — the eager version built the whole thing and then threw it away.
    pub fn auto_dismiss(mut self, cx: &BuildCx, name: &str, ms: f64) -> Toast {
        let now = cx.now_ms();
        let shown_at: lumen_core::Signal<f64> = cx.signal(name, || now);
        let deadline = shown_at.get(cx.runtime()) + ms;
        if now >= deadline {
            // Expired: collapse to nothing (and stop asking for frames).
            self.expired = true;
        } else {
            cx.wake_at(deadline);
        }
        self
    }
}

impl Widget for Toast {
    fn build(self) -> Element {
        let Toast {
            kind,
            title,
            body,
            action,
            expired,
            common,
        } = self;
        if expired {
            return Element::default();
        }

        let mut bar = Element::default().class("bar").class(kind.class());
        bar.background = Some(kind.accent());
        bar.style.width = Dim::px(5.0);
        bar.style.align_self = Some(Align::Stretch);

        let mut title_el = widgets::text(title);
        if let Some(ts) = title_el.text_style_mut() {
            ts.font_size = 15.0;
            ts.weight = 700.0;
        }
        let mut body_el = widgets::text(body);
        if let Some(ts) = body_el.text_style_mut() {
            ts.font_size = 13.0;
            ts.color = Color::srgb8(0x4b, 0x53, 0x60, 0xff);
        }
        let mut col = widgets::column(vec![title_el.class("t-title"), body_el.class("t-body")]);
        col.style.row_gap = Dim::px(3.0);

        let mut children = vec![bar, col];
        if let Some((label, f)) = action {
            let mut btn = widgets::text(label);
            if let Some(ts) = btn.text_style_mut() {
                ts.font_size = 13.0;
                ts.weight = 600.0;
                ts.color = Color::srgb8(0x1a, 0x73, 0xe8, 0xff);
            }
            btn.role = Role::Button;
            btn.focusable = true;
            btn.actions = vec![Action::Click, Action::Focus];
            btn.on_click = Some(f);
            btn.style.flex_shrink = 0.0;
            children.push(btn);
        }

        let mut row = widgets::row(children).class("toast").class(kind.class());
        row.role = Role::Alert;
        row.background = Some(Color::srgb8(0xff, 0xff, 0xff, 0xff));
        row.corner_radius = 10.0;
        row.shadow = Some(crate::element::Shadow::soft());
        row.style.column_gap = Dim::px(14.0);
        row.style.align_items = Some(Align::Stretch);
        row.style.padding = Edges {
            left: Dim::px(14.0),
            right: Dim::px(18.0),
            top: Dim::px(13.0),
            bottom: Dim::px(13.0),
        };
        row.style.width = Dim::px(360.0);
        common.apply(&mut row);
        row
    }
}

impl_widget!(Toast);

/// An indeterminate progress spinner (canvas arc, `cx.animate()`-driven).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Spinner, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Spinner::new(cx, 32.0).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 72.0, 72.0, "spinner");
/// ```
///
/// Renders:
///
/// ![Spinner example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/spinner.png)
///
/// The picture above is `src/doc_shots/spinner.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Spinner {
    diameter: f64,
    color: Color,
    /// The rotation phase, sampled where the `BuildCx` is (a tracked clock read).
    t: f64,
    common: Common,
}

impl Spinner {
    /// An indeterminate spinner in the theme accent.
    pub fn new(cx: &BuildCx, diameter: f64) -> Spinner {
        Spinner::colored(cx, diameter, crate::theme::accent())
    }

    /// An indeterminate spinner in `color`.
    pub fn colored(cx: &BuildCx, diameter: f64, color: Color) -> Spinner {
        cx.animate();
        Spinner {
            diameter,
            color,
            t: cx.now_ms() / 1000.0,
            common: Common::default(),
        }
    }
}

impl Widget for Spinner {
    fn build(self) -> Element {
        let Spinner {
            diameter,
            color,
            t,
            common,
        } = self;
        let mut el = widgets::canvas(diameter, diameter, move |f, size| {
            use kurbo::{Arc, Circle, Point, Shape, Vec2};
            let c = Point::new(size.width / 2.0, size.height / 2.0);
            let r = size.width.min(size.height) / 2.0 - 3.0;
            let track = Color::srgb8(0xe3, 0xe6, 0xeb, 0xff);
            let stroke = (size.width / 12.0).clamp(2.0, 6.0);
            f.stroke(&Circle::new(c, r).to_path(0.1), track, stroke);
            let start = (t * 2.4) % std::f64::consts::TAU;
            let arc =
                Arc::new(c, Vec2::new(r, r), start, std::f64::consts::TAU * 0.78, 0.0).to_path(0.1);
            f.stroke(&arc, color, stroke);
        });
        el = el.class("spinner");
        el.role = Role::Progress;
        el.label = "loading".to_string();
        common.apply(&mut el);
        el
    }
}

impl_widget!(Spinner);

/// A compact pill label, optionally removable.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Chip, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Chip::new("Filter").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 120.0, 52.0, "chip");
/// ```
///
/// Renders:
///
/// ![Chip example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/chip.png)
///
/// The picture above is `src/doc_shots/chip.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Chip {
    label: crate::Text,
    /// `(on, handler)` when the chip is selectable.
    selected: Option<(bool, Rc<dyn Fn(&lumen_core::state::Runtime)>)>,
    /// A leading glyph.
    icon: Option<String>,
    /// A trailing remove (×) affordance.
    on_remove: Option<Rc<dyn Fn(&lumen_core::state::Runtime)>>,
    common: Common,
}

impl Chip {
    /// A pill chip.
    pub fn new(label: impl Into<crate::Text>) -> Chip {
        Chip {
            label: label.into(),
            selected: None,
            icon: None,
            on_remove: None,
            common: Common::default(),
        }
    }

    /// Make the chip selectable, reporting `Selected` while on and calling `f`
    /// when toggled — Material's *filter* and *choice* chips.
    ///
    /// Selection is visual **and** semantic, so the agent and assistive tech can
    /// tell which filters are active.
    pub fn selected(mut self, on: bool, f: impl Fn(&lumen_core::state::Runtime) + 'static) -> Self {
        self.selected = Some((on, Rc::new(f)));
        self
    }

    /// Add a leading icon (Material's *input* chip with an avatar/icon).
    pub fn icon(mut self, glyph: &str) -> Self {
        self.icon = Some(glyph.to_string());
        self
    }

    /// Add a remove (×) affordance calling `f` when clicked.
    pub fn on_remove(mut self, f: impl Fn(&lumen_core::state::Runtime) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }
}

impl Widget for Chip {
    fn build(self) -> Element {
        let Chip {
            label,
            selected,
            icon,
            on_remove,
            common,
        } = self;
        let on = matches!(selected, Some((true, _)));

        let mut text = widgets::text(label);
        if let Some(ts) = text.text_style_mut() {
            ts.font_size = 12.0;
            // Selection recolours *the label*, by name. The eager version
            // recoloured `children.first_mut()`, which was the icon whenever
            // `.icon()` ran before `.selected()` — an order dependency that
            // cannot arise once the children are assembled in one place.
            ts.color = if on {
                Color::srgb8(0x0b, 0x47, 0xa1, 0xff)
            } else {
                Color::srgb8(0x1c, 0x22, 0x30, 0xff)
            };
        }

        let mut children = Vec::with_capacity(
            1 + usize::from(icon.is_some()) + usize::from(on_remove.is_some()),
        );
        if let Some(glyph) = icon {
            let mut ic: Element = crate::Icon::new(&glyph).into();
            ic.style.width = Dim::px(14.0);
            ic.style.height = Dim::px(14.0);
            children.push(ic);
        }
        children.push(text);
        if let Some(f) = on_remove {
            let mut x = widgets::text("×");
            if let Some(ts) = x.text_style_mut() {
                ts.font_size = 13.0;
                ts.color = Color::srgb8(0x6b, 0x72, 0x80, 0xff);
            }
            x.role = Role::Button;
            x.label = "remove".to_string();
            x.focusable = true;
            x.on_click = Some(f);
            children.push(x);
        }

        let mut row = widgets::row(children).class("chip");
        row.background = Some(if on {
            Color::srgb8(0xd7, 0xe6, 0xff, 0xff)
        } else {
            Color::srgb8(0xed, 0xf0, 0xf4, 0xff)
        });
        row.corner_radius = 999.0;
        row.style.align_items = Some(Align::Center);
        row.style.column_gap = Dim::px(6.0);
        row.style.padding = Edges {
            left: Dim::px(10.0),
            right: Dim::px(10.0),
            top: Dim::px(4.0),
            bottom: Dim::px(4.0),
        };
        if let Some((on, f)) = selected {
            row.role = Role::Button;
            row.focusable = true;
            // Built from `widgets::row`, which marks itself structural and would
            // splice the node — and its id — out of the semantic tree. A
            // selectable chip IS the control, so it has to be addressable.
            row.elide_semantics = false;
            row.actions = vec![Action::Click, Action::Focus];
            // `Selected` or nothing — the closed state vocabulary has no
            // `Unselected`, and the rest of the widget set uses the same
            // convention.
            row.states = if on { vec![SemState::Selected] } else { vec![] };
            row.on_click = Some(f);
        }
        common.apply(&mut row);
        row
    }
}

impl_widget!(Chip);
