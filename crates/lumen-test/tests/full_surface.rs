//! T2.1 acceptance: the extended lumen-test surface (05 §2) — drag/set_value,
//! value/state/bounds/style/count assertions, focus, per-test theme.

use kurbo::Size;
use lumen_test::{block_on, expect, TestApp};
use lumen_widgets::{widgets, widgets_m1, App};

fn demo() -> App {
    App::new(|cx| {
        widgets::column(vec![
            widgets::button("A", |_| {}).id("a"),
            widgets::button("B", |_| {}).id("b"),
            widgets_m1::switch(cx, "chk", "Agree").id("chk"),
            widgets::slider(cx, "sld", 0.0, 100.0).id("sld"),
        ])
    })
    .stylesheet("#a { background: #ff0000ff; }")
}

#[test]
fn count_and_style_assertions() {
    block_on(async {
        let app = TestApp::new(demo());
        expect(app.locator("button"))
            .to_have_count(2)
            .await
            .unwrap();
        expect(app.locator("#a"))
            .to_have_style("background", serde_json::json!("#ff0000ff"))
            .await
            .unwrap();
    });
}

#[test]
fn state_and_value_assertions() {
    block_on(async {
        let app = TestApp::new(demo());
        app.locator("#chk").click().await.unwrap();
        expect(app.locator("#chk"))
            .to_have_state("checked")
            .await
            .unwrap();

        // drag the slider to its middle -> value 50
        app.locator("#sld").set_value(0.5).await.unwrap();
        expect(app.locator("#sld"))
            .to_have_value("50")
            .await
            .unwrap();
    });
}

#[test]
fn bounds_and_drag() {
    block_on(async {
        let app = TestApp::new(demo());
        let b = app.locator("#a").bounds().await.unwrap();
        assert!(b.width() > 0.0);
        expect(app.locator("#a"))
            .to_have_bounds_within(b, 0.5)
            .await
            .unwrap();
        // drag from A to B (smoke: no panic, pumps)
        app.locator("#a").drag_to(&app.locator("#b")).await.unwrap();
    });
}

#[test]
fn focus_via_tab() {
    block_on(async {
        let app = TestApp::new(demo());
        app.locator("#a").press("Tab").await.unwrap();
        expect(app.locator("#a")).to_be_focused().await.unwrap();
    });
}

#[test]
fn per_test_theme() {
    block_on(async {
        // theme-scoped token drives the title color
        let themed = || {
            App::new(|_| widgets::text("Hi").id("t")).stylesheet(
                "@theme light { fg: #000000ff; } @theme dark { fg: #ffffffff; } #t { color: $fg; }",
            )
        };
        let light = TestApp::with_options(themed(), Size::new(120.0, 40.0), "light");
        expect(light.locator("#t"))
            .to_have_style("color", serde_json::json!("#000000ff"))
            .await
            .unwrap();
        let dark = TestApp::with_options(themed(), Size::new(120.0, 40.0), "dark");
        expect(dark.locator("#t"))
            .to_have_style("color", serde_json::json!("#ffffffff"))
            .await
            .unwrap();
    });
}
