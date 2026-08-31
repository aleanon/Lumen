//! pokedex — M.5 (ADR-M2): the bring-your-own-client pattern. The app takes
//! its transport as a PARAMETER; the live runner (win.rs) hands it `ureq`
//! (dev-dep), tests hand it canned JSON — the framework ships no HTTP.
use lumen_core::tasks::MaybeSend;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Stack};

/// Build the app around an injected transport: `fetch(name) -> raw JSON`.
pub fn app_with(
    fetch: impl Fn(&str) -> Result<String, String> + MaybeSend + Clone + 'static,
) -> App {
    App::view(move |cx| build(cx, fetch.clone()))
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
) -> impl lumen_widgets::Direct {
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

    // E2b: statement form. `query` stays keyed (the field widget owns it,
    // D1) and the resource flow above is untouched — only the container
    // changes: its whole `LayoutStyle` literal is now Stack modifiers.
    let field = widgets::text_field_basic(cx, "query", &q).id("query-input");
    Stack::column(move |c| {
        c.child(widgets::text("Pokédex (bring-your-own-client)").id("title"));
        c.child(field);
        c.child(body);
    })
    .width(Dim::pct(1.0))
    .height(Dim::pct(1.0))
    .centered()
    .gap(12.0)
    .id("page")
}
