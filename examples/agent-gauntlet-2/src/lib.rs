//! M7-exit (2.0) gauntlet app: a production-grade screen integrating every
//! milestone — localized (i18n/RTL), accessible (WCAG-clean), plugin-extended
//! (third-party rating), media (SVG), and a validated form — plus an injectable
//! regression the agent auto-repairs. Wasm-compatible (no GPU shader).

use lumen_widgets::forms::{form_field, Validator};
use lumen_widgets::i18n::{Catalog, Locale};
use lumen_widgets::{widgets, App, BuildCx, Element};

const LOGO: &str =
    "<svg width=\"32\" height=\"32\"><circle cx=\"16\" cy=\"16\" r=\"14\" fill=\"#1a73e8\"/></svg>";

fn catalog() -> Catalog {
    let mut c = Catalog::new().with_fallback(Locale::new("en"));
    c.insert(&Locale::new("en"), "title", "Account");
    c.insert(&Locale::new("ar"), "title", "الحساب");
    c
}

/// Build the production app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let locale = cx.signal("locale", || Locale::new("en"));
    let bug = cx.signal("bug", || true);
    let loc = locale.get(cx.runtime());
    let buggy = bug.get(cx.runtime());

    let logo = widgets::image(lumen_render::svg::render(
        LOGO,
        32,
        32,
        lumen_core::Color::WHITE,
    ))
    .id("logo");

    let header = widgets::row(vec![
        logo,
        widgets::text(catalog().t(&loc, "title", &[])).id("title"),
    ]);

    let lang = widgets::button("Language", move |rt| {
        locale.update(rt, |l| {
            *l = if l.is_rtl() {
                Locale::new("en")
            } else {
                Locale::new("ar")
            }
        })
    })
    .id("lang");

    let fix = widgets::button("Fix", move |rt| bug.set(rt, false)).id("fix");

    let mut kids = vec![
        header,
        widget_rating::rating(cx, "stars", 5),
        form_field(
            cx,
            "email",
            "Email",
            vec![Validator::Required, Validator::Email],
        ),
        lang,
        fix,
    ];

    // Injected regression: a too-small fixed box overflowing its child (W0103).
    if buggy {
        kids.push(
            Element {
                role: lumen_core::semantics::Role::Group,
                style: lumen_layout::LayoutStyle {
                    width: lumen_layout::Dim::px(30.0),
                    height: lumen_layout::Dim::px(14.0),
                    ..Default::default()
                },
                children: vec![Element {
                    role: lumen_core::semantics::Role::Text,
                    label: "overflow".into(),
                    style: lumen_layout::LayoutStyle {
                        min_width: lumen_layout::Dim::px(180.0),
                        ..Default::default()
                    },
                    ..Element::default()
                }
                .id("bug-child")],
                ..Element::default()
            }
            .id("bug-box"),
        );
    }

    widgets::column(kids).id("root")
}
