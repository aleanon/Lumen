//! data — the async/data layer in action. `cx.resource_blocking` loads a
//! "profile" off the UI thread (here a simulated lookup keyed by an id); the
//! result lands in app state. Refresh bumps the id → the resource refetches,
//! keeping the previous value visible while it reloads (stale-while-revalidate).
//!
//! Runs on the default inline executor headless (`just render data` settles it),
//! and on a real thread pool in a window (`just run data`).
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element, TaskError};

use lumen_layout::{Align, Dim, Edges};

/// Build the data app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// A simulated profile lookup (stands in for an HTTP/DB call — the transport is
/// whatever you call inside the fetcher; the data layer is transport-agnostic).
fn fetch_profile(id: i32) -> Result<(String, u32), TaskError> {
    const NAMES: &[&str] = &[
        "Ada Lovelace",
        "Alan Turing",
        "Grace Hopper",
        "Edsger Dijkstra",
    ];
    let name = NAMES[(id as usize - 1) % NAMES.len()];
    Ok((name.to_string(), (id as u32) * 7 + 5))
}

/// A labelled value row.
fn row(label: &str, value: Element) -> Element {
    let mut l = txt(label, 12.0, 700.0).class("label");
    l.style.width = Dim::px(72.0);
    let mut r = widgets::row(vec![l, value]);
    r.style.column_gap = Dim::px(12.0);
    r.style.align_items = Some(Align::Center);
    r.style.padding = Edges {
        left: Dim::px(14.0),
        right: Dim::px(14.0),
        top: Dim::px(11.0),
        bottom: Dim::px(11.0),
    };
    r.style.width = Dim::pct(1.0);
    r = r.class("row");
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let id = cx.signal("id", || 1i32);
    let cur = id.get(cx.runtime());

    // The headline: a resource keyed on `id`. Refetches when `id` changes.
    let profile =
        cx.resource_blocking::<(String, u32), TaskError, _>("profile", cur, fetch_profile);

    let name = profile
        .value
        .as_ref()
        .map(|(n, _)| n.clone())
        .unwrap_or_else(|| "—".into());
    let repos = profile
        .value
        .as_ref()
        .map(|(_, r)| r.to_string())
        .unwrap_or_else(|| "—".into());

    // Loading indicator (independent of the value — stale data stays visible).
    let status = if profile.loading {
        txt("loading…", 12.0, 700.0).class("loading")
    } else {
        let mut b = txt("ready", 11.0, 800.0).class("badge");
        b.style.padding = Edges {
            left: Dim::px(10.0),
            right: Dim::px(10.0),
            top: Dim::px(4.0),
            bottom: Dim::px(4.0),
        };
        b
    };

    let header = {
        let mut r = widgets::row(vec![txt("Profile", 24.0, 800.0).class("title"), status]);
        r.style.width = Dim::pct(1.0);
        r.style.align_items = Some(Align::Center);
        r.style.justify_content = Some(Align::SpaceBetween);
        r
    };

    let refresh = {
        let mut b = widgets::button("Load next", move |rt| id.update(rt, |v| *v += 1)).class("btn");
        if let Some(ts) = b.text_style_mut() {
            ts.font_size = 14.0;
            ts.weight = 700.0;
        }
        b.style.padding = Edges {
            left: Dim::px(18.0),
            right: Dim::px(18.0),
            top: Dim::px(11.0),
            bottom: Dim::px(11.0),
        };
        b.id("refresh")
    };

    let mut card = widgets::column(vec![
        header,
        txt("Loaded off the UI thread via cx.resource.", 14.0, 400.0).class("subtitle"),
        row("USER", txt(format!("#{cur}"), 15.0, 700.0).class("value")),
        row("NAME", txt(name, 15.0, 600.0).class("value")),
        row("REPOS", txt(repos, 15.0, 600.0).class("value")),
        refresh,
    ])
    .id("card");
    card.style.align_items = Some(Align::Start);
    card.style.row_gap = Dim::px(12.0);
    card.style.padding = Edges::all(Dim::px(28.0));
    card.style.width = Dim::px(360.0);
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
