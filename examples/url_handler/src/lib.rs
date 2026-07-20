//! url_handler — M.6: deep links through `nav::Router`. The OS registration
//! (`x-scheme-handler/lumen`) belongs to packaging (E.1); this app shows the
//! routing half: an incoming `lumen://…` URL is parsed, the scheme stripped,
//! and the router replaces its stack from the path — screen + parameter
//! arrive like any state change.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::nav::Router;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the url_handler app.
pub fn main_app() -> App {
    App::new(build)
}

/// Strip the scheme and hand the path to the router (the piece an OS URL
/// activation would call).
pub fn open_url(rt: &lumen_core::state::Runtime, url: &str) {
    let path = url.strip_prefix("lumen://").unwrap_or(url).to_string();
    let router = rt.signal("router", || Router::new("home"));
    router.update(rt, move |r| r.deep_link(&path.replace('/', ":")));
}

fn build(cx: &mut BuildCx) -> Element {
    let router = cx.signal("router", || Router::new("home"));
    let current = router.get(cx.runtime()).current().to_string();

    let screen = match current.split(':').next().unwrap_or("home") {
        "settings" => widgets::text("Settings screen").id("screen-settings"),
        "profile" => {
            let who = current.split(':').nth(1).unwrap_or("anon").to_string();
            widgets::text(format!("Profile: {who}")).id("screen-profile")
        }
        _ => widgets::text("Home").id("screen-home"),
    };

    let mut links = widgets::row(vec![
        widgets::button("lumen://settings", |rt| open_url(rt, "lumen://settings"))
            .id("link-settings"),
        widgets::button("lumen://profile/ada", |rt| {
            open_url(rt, "lumen://profile/ada")
        })
        .id("link-profile"),
    ]);
    links.style.column_gap = Dim::px(8.0);

    let mut col = widgets::column(vec![
        widgets::text(format!("route: {current}")).id("route"),
        screen,
        links,
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
