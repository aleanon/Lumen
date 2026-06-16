//! T5.3 acceptance: message catalogs (interpolation, plurals, missing→W0401),
//! locale number formatting, and RTL layout mirroring.

use kurbo::Size;
use lumen_widgets::i18n::{format_number, Arg, Catalog, Locale};
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

fn en() -> Locale {
    Locale::new("en")
}
fn ar() -> Locale {
    Locale::new("ar")
}

fn catalog() -> Catalog {
    let mut c = Catalog::new().with_fallback(en());
    c.insert(&en(), "greeting", "Hello, {name}!");
    c.insert(&ar(), "greeting", "مرحبا، {name}");
    c.insert(
        &en(),
        "items",
        "{count, plural, one {# item} other {# items}}",
    );
    c.insert(&en(), "only_en", "English only");
    c
}

#[test]
fn interpolation_and_translation() {
    let c = catalog();
    assert_eq!(
        c.t(&en(), "greeting", &[("name", Arg::Str("Ada"))]),
        "Hello, Ada!"
    );
    assert_eq!(
        c.t(&ar(), "greeting", &[("name", Arg::Str("علي"))]),
        "مرحبا، علي"
    );
}

#[test]
fn plurals_select_by_count() {
    let c = catalog();
    assert_eq!(c.t(&en(), "items", &[("count", Arg::Int(1))]), "1 item");
    assert_eq!(c.t(&en(), "items", &[("count", Arg::Int(5))]), "5 items");
}

#[test]
fn missing_key_falls_back_then_warns() {
    let c = catalog();
    // Falls back to en for ar.
    assert_eq!(c.t(&ar(), "only_en", &[]), "English only");
    // Truly missing → marker + W0401.
    let (text, diag) = c.translate(&en(), "nope", &[]);
    assert_eq!(text, "⟨nope⟩");
    assert_eq!(diag.unwrap().code, "W0401");
}

#[test]
fn number_formatting_is_locale_aware() {
    assert_eq!(format_number(1234567, &en()), "1,234,567");
    assert_eq!(format_number(1234567, &Locale::new("de")), "1.234.567");
    assert_eq!(format_number(1234567, &Locale::new("ja")), "1,234,567");
    assert_eq!(format_number(-42, &en()), "-42");
}

#[test]
fn arabic_plural_categories() {
    let ar = ar();
    assert_eq!(ar.plural_category(0), "zero");
    assert_eq!(ar.plural_category(1), "one");
    assert_eq!(ar.plural_category(2), "two");
    assert_eq!(ar.plural_category(5), "few");
    assert!(ar.is_rtl());
    assert!(!en().is_rtl());
}

fn row_app(cx: &mut BuildCx) -> Element {
    let _ = cx;
    widgets::row(vec![
        widgets::text("A").id("a"),
        widgets::text("B").id("b"),
        widgets::text("C").id("c"),
    ])
}

fn x_of(h: &Headless, id: &str) -> f64 {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<f64> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds.x0);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&h.semantics_doc().root.elided(), id).unwrap()
}

#[test]
fn rtl_mirrors_the_layout() {
    let mut h = App::new(row_app).run_headless(Size::new(300.0, 40.0));
    // LTR: A left of B left of C.
    assert!(x_of(&h, "a") < x_of(&h, "b") && x_of(&h, "b") < x_of(&h, "c"));

    // RTL: the row mirrors — A is now rightmost.
    h.set_rtl(true);
    assert!(h.is_rtl());
    assert!(
        x_of(&h, "a") > x_of(&h, "b") && x_of(&h, "b") > x_of(&h, "c"),
        "row reads right-to-left"
    );
}
