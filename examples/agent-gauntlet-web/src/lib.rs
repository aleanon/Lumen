//! M5-exit gauntlet app: a **localized, routed, form-driven CRUD** contacts app
//! exercising every M5 capability — i18n/RTL, routing + back stack, a validated
//! form, and undo/redo — built once and run on desktop + web + Android.

use lumen_widgets::forms::{form_field, validate, Validator};
use lumen_widgets::i18n::{Catalog, Locale};
use lumen_widgets::nav::Router;
use lumen_widgets::undo::History;
use lumen_widgets::{widgets, widgets_m1, App, BuildCx, Element};

fn catalog() -> Catalog {
    let mut c = Catalog::new().with_fallback(Locale::new("en"));
    c.insert(&Locale::new("en"), "title", "Contacts");
    c.insert(&Locale::new("ar"), "title", "جهات الاتصال");
    c.insert(&Locale::new("en"), "add", "Add");
    c.insert(&Locale::new("ar"), "add", "أضف");
    c
}

/// Build the contacts app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let router = cx.signal("router", || Router::new("list"));
    let contacts = cx.signal("contacts", || History::new(Vec::<String>::new()));
    let locale = cx.signal("locale", || Locale::new("en"));

    let loc = locale.get(cx.runtime());
    let cat = catalog();
    let route = router.get(cx.runtime()).current().to_string();
    let list = contacts.get(cx.runtime()).present().clone();

    let header = widgets::row(vec![
        widgets::text(cat.t(&loc, "title", &[])).id("title"),
        widgets_m1::spacer(),
        widgets::button("EN/AR", move |rt| {
            locale.update(rt, |l| {
                *l = if l.is_rtl() {
                    Locale::new("en")
                } else {
                    Locale::new("ar")
                }
            })
        })
        .id("locale"),
    ]);

    let body = if route == "add" {
        // The "add contact" screen: a validated name field + save/back.
        let name_field = form_field(cx, "newname", "Name", vec![Validator::Required]);
        widgets::column(vec![
            name_field,
            widgets::row(vec![
                commit_button(cx, router, contacts),
                back_button(router),
            ]),
        ])
    } else {
        // The list screen.
        let mut items: Vec<Element> = list
            .iter()
            .enumerate()
            .map(|(i, name)| widgets::text(format!("{}. {name}", i + 1)).id(format!("contact-{i}")))
            .collect();
        if items.is_empty() {
            items.push(widgets::text("(no contacts)").id("empty"));
        }
        let add = widgets::button(cat.t(&loc, "add", &[]), move |rt| {
            router.update(rt, |r| r.navigate("add"))
        })
        .id("add");
        let undo = widgets::button("Undo", move |rt| {
            contacts.update(rt, |h| {
                h.undo();
            })
        })
        .id("undo");
        let del = widgets::button("Delete last", move |rt| {
            contacts.update(rt, |h| {
                let mut v = h.present().clone();
                v.pop();
                h.push(v);
            })
        })
        .id("delete");
        let mut col = vec![add, undo, del];
        col.extend(items);
        widgets::column(col)
    };

    widgets::column(vec![header, widgets_m1::divider(), body]).id("root")
}

// Save the new contact (validating) then return to the list.
fn commit_button(
    cx: &BuildCx,
    router: lumen_core::state::Signal<Router>,
    contacts: lumen_core::state::Signal<History<Vec<String>>>,
) -> Element {
    let name = cx.signal("newname", String::new);
    widgets::button("Add contact", move |rt| {
        let value = name.get(rt);
        if validate(&value, &[Validator::Required]).is_none() {
            contacts.update(rt, |h| {
                let mut v = h.present().clone();
                v.push(value.clone());
                h.push(v);
            });
            name.set(rt, String::new());
            router.update(rt, |r| {
                r.back();
            });
        }
    })
    .id("commit")
}

fn back_button(router: lumen_core::state::Signal<Router>) -> Element {
    widgets::button("Back", move |rt| {
        router.update(rt, |r| {
            r.back();
        })
    })
    .id("back")
}
