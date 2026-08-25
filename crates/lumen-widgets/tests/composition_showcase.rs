//! What composing widgets looks like under the `Widget` trait.
//!
//! Four patterns, in order of how much the trait changed them: ordinary
//! composition (not at all), generic helpers over `W: Widget` (new), a
//! composite that holds other widgets *unbuilt* and edits them before they
//! lower (new, and the reason the trait is interesting), and a foreign widget
//! that inherits the universal vocabulary (new).

use lumen_core::geometry::Size;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::Align;
use lumen_widgets::{
    impl_widget, widgets, App, BuildCx, Button, Card, CheckBox, Common, Container, Element, Label,
    ProgressBar, Widget,
};

fn ink() -> Color {
    Color::srgb8(0x1c, 0x22, 0x30, 0xff)
}
fn muted() -> Color {
    Color::srgb8(0x6b, 0x74, 0x88, 0xff)
}
fn ok() -> Color {
    Color::srgb8(0x18, 0x8a, 0x42, 0xff)
}
fn danger() -> Color {
    Color::srgb8(0xd3, 0x2f, 0x2f, 0xff)
}

// ---------------------------------------------------------------------------
// 1. A foreign widget — defined here, not in lumen-widgets.
// ---------------------------------------------------------------------------

/// A labelled bar. It stores three fields and a `Common`; it is not an
/// `Element` until someone asks for one.
struct Gauge {
    fraction: f64,
    tint: Color,
    common: Common,
}

impl Gauge {
    fn new(fraction: f64) -> Gauge {
        Gauge {
            fraction: fraction.clamp(0.0, 1.0),
            tint: ok(),
            common: Common::default(),
        }
    }

    /// Recolour the bar. A field write — there is no node to reach into yet.
    fn danger(mut self) -> Gauge {
        self.tint = danger();
        self
    }
}

impl Widget for Gauge {
    fn build(self) -> Element {
        let Gauge {
            fraction,
            tint,
            common,
        } = self;
        let pct = format!("{:.0}%", fraction * 100.0);
        let mut el: Element = Container::new(vec![
            ProgressBar::new(fraction)
                .fill_color(tint)
                .width(120.0)
                .height(8.0)
                .into(),
            Label::new(pct.clone()).size(12.0).color(muted()).into(),
        ])
        .row()
        .gap(10.0)
        .align(Align::Center)
        .into();
        // Give it a real role and value so the agent and assistive tech see a
        // gauge rather than an anonymous row.
        el.role = Role::Progress;
        el.value = Some(pct);
        common.apply(&mut el);
        el
    }
}

// `impl_widget!` is exported, so a foreign widget gets `.id()`, `.class()`,
// `.background()`, `.style()`, `.css()`, `.disabled()` and `From<Gauge> for
// Element` — the same vocabulary every built-in has.
impl_widget!(Gauge);

// ---------------------------------------------------------------------------
// 2. A generic helper — works for any widget, built-in or foreign.
// ---------------------------------------------------------------------------

/// A caption beside a control. `W: Widget` accepts a `Label`, a `Button`, a
/// `CheckBox` or the `Gauge` above without knowing which.
fn field<W: Widget>(caption: &str, control: W) -> Container {
    Container::new(vec![
        Label::new(caption.to_string())
            .size(13.0)
            .color(muted())
            .width(96.0)
            .into(),
        control.build(),
    ])
    .row()
    .gap(12.0)
    .align(Align::Center)
}

// ---------------------------------------------------------------------------
// 3. A composite that holds other widgets UNBUILT.
// ---------------------------------------------------------------------------

/// A metric row: a caption, a gauge, and an optional action button.
///
/// It keeps the `Gauge` and the `Button` as **widget values**, not `Element`s.
/// That is what lets `.critical()` reach into the gauge's *intent* and recolour
/// it — under the eager model those children would already be materialized
/// nodes, and the only way in would be to mutate node fields by position.
struct Metric {
    caption: String,
    gauge: Gauge,
    action: Option<Button>,
    common: Common,
}

impl Metric {
    fn new(caption: &str, fraction: f64) -> Metric {
        Metric {
            caption: caption.to_string(),
            gauge: Gauge::new(fraction),
            action: None,
            common: Common::default(),
        }
    }

    /// Attach an action. The `Button` is stored, not lowered.
    fn action(mut self, label: &str, f: impl Fn(&lumen_core::state::Runtime) + 'static) -> Metric {
        self.action = Some(Button::new(label.to_string()).ghost().on_press(f));
        self
    }

    /// Escalate the whole row — recolours the gauge and quietens the action,
    /// by editing the widgets it is holding rather than the nodes they become.
    fn critical(mut self) -> Metric {
        self.gauge = self.gauge.danger();
        self.action = self.action.map(|b| b.text_color(danger()));
        self
    }
}

impl Widget for Metric {
    fn build(self) -> Element {
        let Metric {
            caption,
            gauge,
            action,
            common,
        } = self;
        let mut kids = vec![
            Label::new(caption).size(13.0).color(ink()).width(96.0).into(),
            gauge.build(),
        ];
        if let Some(button) = action {
            kids.push(widgets::spacer());
            kids.push(button.build());
        }
        let mut el: Element = Container::new(kids)
            .row()
            .gap(12.0)
            .align(Align::Center)
            .width(360.0)
            .into();
        common.apply(&mut el);
        el
    }
}

impl_widget!(Metric);

// ---------------------------------------------------------------------------
// 4. The screen: everything above composed together.
// ---------------------------------------------------------------------------

fn view(cx: &mut BuildCx) -> Element {
    let card = Card::new(vec![
        // Ordinary composition — unchanged by the trait. `.into()` still works
        // because `From<W> for Element` is now a call to `Widget::build`.
        Metric::new("api", 0.34).action("Restart", |_| {}).id("m-api").into(),
        Metric::new("queue", 0.91)
            .action("Drain", |_| {})
            .critical()
            .id("m-queue")
            .into(),
        widgets::divider(),
        // A generic helper, three different widget types through one signature.
        field("uptime", Label::new("99.98%").size(13.0).color(ink())).into(),
        field("headroom", Gauge::new(0.62)).into(),
        field("alerts", CheckBox::new(cx, "alerts", "page on-call")).into(),
        // A foreign widget carrying a universal modifier the framework defined.
        field("retired", Gauge::new(0.5).disabled(true)).into(),
    ])
    .title("Service health")
    .into();

    lumen_widgets::centered(cx, card)
}

#[test]
fn the_composition_builds_and_is_addressable() {
    let mut h = App::new(view).run_headless(Size::new(460.0, 400.0));
    h.pump();

    // Foreign widgets and composites are addressable exactly like built-ins.
    assert!(h.node_bounds_by_id("m-api").is_some(), "composite is in the tree");
    assert!(h.node_bounds_by_id("m-queue").is_some());
    h.assert_view_coherent();

    // `.critical()` reached into the held Gauge before it lowered.
    let doc = format!("{:?}", h.semantics_doc());
    assert!(doc.contains("91%"), "the escalated gauge reports its value: {doc}");

    if std::env::var_os("LUMEN_WRITE_SHOWCASE").is_some() {
        std::fs::write("/tmp/composition.png", h.screenshot().to_png()).unwrap();
    }
}
