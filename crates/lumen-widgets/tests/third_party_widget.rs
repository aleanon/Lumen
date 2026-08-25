//! Can code *outside* the framework implement `Widget` and compose with the
//! built-ins on equal terms? That is the extensibility question the trait
//! exists to answer, so it gets a test rather than an assurance.

use lumen_widgets::{impl_widget, widgets, App, Button, Common, Container, Element, Label, Widget};
use lumen_core::geometry::Size;
use lumen_core::semantics::Role;

/// A widget defined here, not in `lumen-widgets`: a labelled statistic.
/// It stores only what it needs — two strings and a flag — and lowers on demand.
struct Stat {
    caption: String,
    value: String,
    emphasized: bool,
}

impl Stat {
    fn new(caption: &str, value: &str) -> Stat {
        Stat {
            caption: caption.to_string(),
            value: value.to_string(),
            emphasized: false,
        }
    }
    fn emphasized(mut self) -> Stat {
        self.emphasized = true;
        self
    }
}

impl Widget for Stat {
    fn build(self) -> Element {
        let Stat {
            caption,
            value,
            emphasized,
        } = self;
        let mut el: Element = Container::new(vec![
            Label::new(caption).size(11.0).into(),
            Label::new(value)
                .size(if emphasized { 24.0 } else { 16.0 })
                .bold()
                .into(),
        ])
        .gap(2.0)
        .into();
        el.role = Role::Group;
        el
    }
}

#[test]
fn a_foreign_widget_composes_with_the_built_ins() {
    // No `impl From<Stat> for Element` was written here — only `Widget`.
    let app = App::new(|_cx| {
        widgets::column(vec![
            Stat::new("Requests", "1,204").emphasized().build(),
            Stat::new("Errors", "3").build(),
            Button::new("Refresh").on_press(|_| {}).into(),
        ])
    });
    let mut h = app.run_headless(Size::new(300.0, 200.0));
    h.pump();

    let text = format!("{:?}", h.semantics_doc());
    assert!(text.contains("Requests"), "foreign widget reached semantics: {text}");
    assert!(text.contains("1,204"));
    assert!(text.contains("Refresh"), "built-in still present alongside it");
    h.assert_view_coherent();
}

/// A foreign widget can also *hold* built-in widget values unbuilt, and lower
/// them itself — the composition property that makes the trait worth having.
struct Labelled {
    label: Label,
    control: Button,
}

impl Widget for Labelled {
    fn build(self) -> Element {
        let Labelled { label, control } = self;
        Container::new(vec![label.build(), control.build()])
            .row()
            .gap(8.0)
            .into()
    }
}

#[test]
fn a_foreign_widget_can_defer_built_in_widgets() {
    let app = App::new(|_cx| {
        Labelled {
            label: Label::new("Name").bold(),
            control: Button::new("Edit").ghost(),
        }
        .build()
    });
    let mut h = app.run_headless(Size::new(300.0, 100.0));
    h.pump();
    let text = format!("{:?}", h.semantics_doc());
    assert!(text.contains("Name") && text.contains("Edit"), "{text}");
}


/// A foreign widget that opts into the shared vocabulary via `impl_widget!`.
///
/// This is the part that decides whether the trait is worth having for
/// extensibility: without the exported macro, `Gauge` would implement `Widget`
/// and still inherit none of `.id()`, `.class()`, `.background()`, `.style()`,
/// `.css()` or `.disabled()` — it could not sit beside a `Button` on equal
/// terms without hand-writing all six and the disabled dimming.
struct Gauge {
    fraction: f64,
    common: Common,
}

impl Gauge {
    fn new(fraction: f64) -> Gauge {
        Gauge {
            fraction: fraction.clamp(0.0, 1.0),
            common: Common::default(),
        }
    }
}

impl Widget for Gauge {
    fn build(self) -> Element {
        let Gauge { fraction, common } = self;
        let mut el: Element = Label::new(format!("{:.0}%", fraction * 100.0)).into();
        el.role = Role::Progress;
        el.value = Some(format!("{:.0}%", fraction * 100.0));
        common.apply(&mut el);
        el
    }
}

impl_widget!(Gauge);

#[test]
fn a_foreign_widget_inherits_the_universal_modifiers() {
    let el: Element = Gauge::new(0.42)
        .id("cpu")
        .class("kpi")
        .background(lumen_core::Color::srgb8(0x11, 0x22, 0x33, 0xff))
        .disabled(true)
        .into();

    assert_eq!(el.id.as_ref().map(|i| i.0.as_str()), Some("cpu"));
    assert!(el.classes.iter().any(|c| c == "kpi"));
    assert!(el.disabled, "disabled reached the node");
    // `disabled` also dims — the behaviour a hand-rolled widget would have to
    // reimplement, and would probably forget.
    let bg = el.background.expect("background applied");
    assert!(bg.r > 0.5 && bg.g > 0.5, "disabled washed the fill toward the page");
    assert_eq!(el.cursor, None, "a disabled control advertises no pointer shape");
}

#[test]
fn a_foreign_widget_sits_in_a_tree_beside_built_ins() {
    let app = App::new(|_cx| {
        widgets::column(vec![
            Gauge::new(0.9).id("cpu").into(),
            Button::new("Reset").on_press(|_| {}).into(),
        ])
    });
    let mut h = app.run_headless(Size::new(240.0, 120.0));
    h.pump();
    assert!(
        h.node_bounds_by_id("cpu").is_some(),
        "the foreign widget is addressable by id, like any built-in"
    );
    h.assert_view_coherent();
}
