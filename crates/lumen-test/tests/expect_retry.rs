//! T.2 (docs/plan-remediation-2026-07.md): every `expect` assertion
//! auto-retries on the virtual clock — a condition that only becomes true
//! after time passes settles without explicit clock control — and the new
//! Locator surface (`right_click`, `type_text`, `to_be_visible`) works.

use lumen_test::{expect, LocatorError, TestApp};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// `#late` appears only once the (virtual) clock passes 200 ms — a
/// time-driven build, so each retry poll's clock advance makes progress.
fn delayed_app(cx: &mut BuildCx) -> Element {
    let now = cx.now_ms();
    if now >= 200.0 {
        widgets::column(vec![widgets::text("here").id("late")])
    } else {
        widgets::text("waiting…").id("early")
    }
}

#[lumen_test::test(app(App::new(delayed_app)))]
async fn every_assertion_retries_on_the_virtual_clock(mut app: TestApp) {
    app.pump_until_idle().await;
    // Each of these is false at t=0 and true at t≥200ms — the retry loop's
    // clock advance (not test code) must get them there.
    expect(app.locator("#late")).to_exist().await.unwrap();
    expect(app.locator("#late")).to_be_visible().await.unwrap();
    expect(app.locator("#late"))
        .to_have_text("here")
        .await
        .unwrap();
    expect(app.locator("#late")).to_have_count(1).await.unwrap();
}

#[lumen_test::test(app(App::new(delayed_app)))]
async fn misses_still_time_out_structuredly(mut app: TestApp) {
    app.pump_until_idle().await;
    let err = expect(app.locator("#never"))
        .to_have_text("nope")
        .await
        .unwrap_err();
    assert_eq!(err, LocatorError::Timeout);
}

fn editor_app(cx: &mut BuildCx) -> Element {
    widgets::column(vec![widgets::text_field_basic(cx, "f", "ab").id("f")])
}

#[lumen_test::test(app(App::new(editor_app)))]
async fn type_text_appends(mut app: TestApp) {
    app.pump_until_idle().await;
    app.locator("#f").type_text("cd").await.unwrap();
    expect(app.locator("#f"))
        .to_have_value("abcd")
        .await
        .unwrap();
}

#[lumen_test::test(app(App::new(editor_app)))]
async fn right_click_is_deliverable(mut app: TestApp) {
    app.pump_until_idle().await;
    // No context-menu handling exists yet — the assertion is that a
    // secondary-button click synthesizes and routes without disturbing state.
    app.locator("#f").right_click().await.unwrap();
    expect(app.locator("#f")).to_have_value("ab").await.unwrap();
}
