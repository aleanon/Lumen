//! T.1 (docs/plan-remediation-2026-07.md): the `#[lumen_test::test]`
//! attribute — async body over the `main_app()` convention, per-test
//! size/scale/theme, and `platform(...)` ⇒ `#[ignore]`.

use lumen_test::{expect, TestApp};
use lumen_widgets::{widgets, App};

fn main_app() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("Count: {v}")).id("count"),
            widgets::button("+1", move |rt| count.update(rt, |c| *c += 1)).id("increment"),
        ])
    })
}

#[lumen_test::test]
async fn default_config_runs(mut app: TestApp) {
    app.pump_until_idle().await;
    app.locator("#increment").click().await.unwrap();
    expect(app.locator("#count"))
        .to_have_text("Count: 1")
        .await
        .unwrap();
}

#[lumen_test::test(size(400, 300), scale(2.0), theme(dark))]
async fn size_scale_and_theme_apply(mut app: TestApp) {
    app.pump_until_idle().await;
    // 400×300 logical at 2× → an 800×600 physical frame.
    let shot = app.screenshot().await;
    assert_eq!((shot.width(), shot.height()), (800, 600));
}

#[lumen_test::test(app(main_app()), size(320, 240))]
async fn explicit_app_expression(mut app: TestApp) {
    app.pump_until_idle().await;
    expect(app.locator("#count"))
        .to_have_text("Count: 0")
        .await
        .unwrap();
}

#[lumen_test::test(platform(android))]
async fn platform_tests_are_ignored(_app: TestApp) {
    // Only reachable with `--ignored` + a platform runner.
    panic!("must not run in the default suite");
}
