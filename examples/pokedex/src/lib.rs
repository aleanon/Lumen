//! pokedex — M.5 (ADR-M2): the bring-your-own-client pattern. The app takes
//! its transport as a PARAMETER; the live runner (win.rs) hands it `ureq`
//! (dev-dep), tests hand it canned JSON — the framework ships no HTTP.
use lumen_core::tasks::MaybeSend;
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the app around an injected transport: `fetch(name) -> raw JSON`.
pub fn app_with(
    fetch: impl Fn(&str) -> Result<String, String> + MaybeSend + Clone + 'static,
) -> App {
    App::new(move |cx| build(cx, fetch.clone()))
}

/// Extract `"name"` and the first `"type"` from the (Pokéapi-shaped) JSON —
/// dependency-free string scanning; a real app brings serde.
fn scrape(json: &str, key: &str) -> String {
    json.split(&format!("\"{key}\":\""))
        .nth(1)
        .and_then(|r| r.split('"').next())
        .unwrap_or("?")
        .to_string()
}

fn build(
    cx: &mut BuildCx,
    fetch: impl Fn(&str) -> Result<String, String> + MaybeSend + Clone + 'static,
) -> Element {
    let query = cx.signal("query", || "pikachu".to_string());
    let q = query.get(cx.runtime());
    let f = fetch.clone();
    let r = cx.resource_blocking::<String, String, _>("mon", q.clone(), move |q| {
        f(&format!("https://pokeapi.co/api/v2/pokemon/{q}"))
    });

    let body = if r.loading {
        widgets::text("loading…").id("status")
    } else if let Some(e) = &r.error {
        widgets::text(format!("error: {e}")).id("status")
    } else {
        let json = r.value.unwrap_or_default();
        widgets::text(format!("{} — height {}", scrape(&json, "name"), {
            json.split("\"height\":")
                .nth(1)
                .and_then(|s| s.split(',').next())
                .unwrap_or("?")
                .trim()
                .to_string()
        }))
        .id("mon")
    };

    let mut col = widgets::column(vec![
        widgets::text("Pokédex (bring-your-own-client)").id("title"),
        widgets::text_field_basic(cx, "query", &q).id("query-input"),
        body,
    ])
    .id("page");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(12.0),
        ..LayoutStyle::default()
    };
    col
}
