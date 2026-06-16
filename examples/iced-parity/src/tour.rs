//! tour — a multi-page walkthrough using tabs + a back stack (router).
use lumen_widgets::nav::Router;
use lumen_widgets::{widgets, widgets_m1, App, BuildCx, Element};

/// Build the tour app.
pub fn main_app() -> App {
    App::new(build)
}

const PAGES: &[&str] = &["Welcome", "Widgets", "Layout", "Finish"];

fn build(cx: &mut BuildCx) -> Element {
    let router = cx.signal("router", || Router::new("0"));
    let idx: usize = router.get(cx.runtime()).current().parse().unwrap_or(0);

    let next = widgets::button("Next", move |rt| {
        router.update(rt, |r| {
            let cur: usize = r.current().parse().unwrap_or(0);
            r.navigate((cur + 1).min(PAGES.len() - 1).to_string());
        })
    })
    .id("next");
    let back = widgets::button("Back", move |rt| {
        router.update(rt, |r| {
            r.back();
        })
    })
    .id("back");

    widgets::column(vec![
        widgets::text(format!("Page {}/{}: {}", idx + 1, PAGES.len(), PAGES[idx])).id("page"),
        widgets_m1::divider(),
        widgets::text(format!("This is the {} page of the tour.", PAGES[idx])).id("body"),
        widgets::row(vec![back, next]),
    ])
    .id("root")
}
