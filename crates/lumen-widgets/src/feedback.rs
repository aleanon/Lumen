//! [`Toast`], [`Spinner`], and [`Chip`] (W.1) — promoted from the
//! `examples/toast` and `examples/loading_spinners` prototypes into the
//! library. Colors are built in (per kind) so the widgets work with no
//! stylesheet; the classes stay on the elements for `.lss` overrides.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_core::semantics::Role;
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
/// use lumen_widgets::{App, Toast, ToastKind};
///
/// let app = App::new(|_| Toast::new(ToastKind::Success, "Saved", "Changes stored").into());
/// # lumen_widgets::doc_shot(app, 240.0, 72.0, "toast");
/// ```
pub struct Toast {
    el: Element,
}

impl Toast {
    /// A toast card. `Role::Alert` so agents/screen readers see it announce.
    pub fn new(kind: ToastKind, title: impl Into<String>, body: impl Into<String>) -> Toast {
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

        let mut row = widgets::row(vec![bar, col])
            .class("toast")
            .class(kind.class());
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
        Toast { el: row }
    }
}

impl_common!(Toast);

/// An indeterminate progress spinner (canvas arc, `cx.animate()`-driven).
/// # Example
///
/// ```
/// use lumen_widgets::{App, Spinner};
///
/// let app = App::new(|cx| Spinner::new(cx, 32.0).into());
/// # lumen_widgets::doc_shot(app, 56.0, 56.0, "spinner");
/// ```
pub struct Spinner {
    el: Element,
}

impl Spinner {
    /// A spinner of `diameter` px in the accent color.
    pub fn new(cx: &BuildCx, diameter: f64) -> Spinner {
        Spinner::colored(cx, diameter, crate::theme::accent())
    }

    /// A spinner in an explicit color.
    pub fn colored(cx: &BuildCx, diameter: f64, color: Color) -> Spinner {
        cx.animate();
        let t = cx.now_ms() / 1000.0;
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
        Spinner { el }
    }
}

impl_common!(Spinner);

/// A compact pill label, optionally removable.
/// # Example
///
/// ```
/// use lumen_widgets::{App, Chip};
///
/// let app = App::new(|_| Chip::new("Filter").into());
/// # lumen_widgets::doc_shot(app, 100.0, 40.0, "chip");
/// ```
pub struct Chip {
    el: Element,
}

impl Chip {
    /// A pill chip.
    pub fn new(label: impl Into<String>) -> Chip {
        let mut text = widgets::text(label);
        if let Some(ts) = text.text_style_mut() {
            ts.font_size = 12.0;
            ts.color = Color::srgb8(0x1c, 0x22, 0x30, 0xff);
        }
        let mut row = widgets::row(vec![text]).class("chip");
        row.background = Some(Color::srgb8(0xed, 0xf0, 0xf4, 0xff));
        row.corner_radius = 999.0;
        row.style.align_items = Some(Align::Center);
        row.style.column_gap = Dim::px(6.0);
        row.style.padding = Edges {
            left: Dim::px(10.0),
            right: Dim::px(10.0),
            top: Dim::px(4.0),
            bottom: Dim::px(4.0),
        };
        Chip { el: row }
    }

    /// Add a remove (×) affordance calling `f` when clicked.
    pub fn on_remove(mut self, f: impl Fn(&lumen_core::state::Runtime) + 'static) -> Self {
        let mut x = widgets::text("×");
        if let Some(ts) = x.text_style_mut() {
            ts.font_size = 13.0;
            ts.color = Color::srgb8(0x6b, 0x72, 0x80, 0xff);
        }
        x.role = Role::Button;
        x.label = "remove".to_string();
        x.focusable = true;
        x.on_click = Some(Rc::new(f));
        self.el.children.push(x);
        self
    }
}

impl_common!(Chip);
