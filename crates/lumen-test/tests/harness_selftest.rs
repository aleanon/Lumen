//! T0.9 acceptance: harness self-tests — auto-wait on delayed appearance,
//! Ambiguous-with-candidates on duplicates, golden round-trip, and a full
//! counter click→assert loop.

use lumen::{App, Element};
use lumen_test::{block_on, expect, LocatorError, TestApp};

/// A node `#late` that only appears once virtual time reaches 50 ms.
fn delayed_app() -> App {
    App::new(|cx| {
        let mut kids = vec![Element::text("loading")];
        if cx.now_ms() >= 50.0 {
            kids.push(Element::button("Ready").id("late"));
        }
        Element::column(kids)
    })
}

/// Two buttons → the `button` selector is ambiguous.
fn dup_app() -> App {
    App::new(|_cx| {
        Element::row(vec![
            Element::button("A").id("a"),
            Element::button("B").id("b"),
        ])
    })
}

/// A counter: `#count` shows the value, `#inc` increments it.
fn counter_app() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let value = count.get(cx.runtime());
        Element::column(vec![
            Element::text(format!("{value}")).id("count"),
            Element::button("+1")
                .id("inc")
                .on_click(move |rt| count.update(rt, |c| *c += 1)),
        ])
    })
}

#[test]
fn auto_wait_succeeds_on_delayed_appearance() {
    block_on(async {
        let app = TestApp::new(delayed_app());
        // Not present at t=0; auto-wait advances the virtual clock until it is.
        assert_eq!(app.locator("#late").count().await, 0);
        expect(app.locator("#late"))
            .to_exist()
            .await
            .expect("#late should appear after auto-wait");
    });
}

#[test]
fn ambiguous_selector_reports_candidates() {
    block_on(async {
        let app = TestApp::new(dup_app());
        let err = app.locator("button").click().await.unwrap_err();
        match err {
            LocatorError::Ambiguous { candidates } => {
                assert_eq!(candidates.len(), 2, "two buttons should be candidates");
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    });
}

#[test]
fn not_found_after_timeout() {
    block_on(async {
        let app = TestApp::new(dup_app());
        let err = app.locator("#missing").click().await.unwrap_err();
        assert!(matches!(err, LocatorError::NotFound { .. }));
    });
}

#[test]
fn counter_click_updates_label() {
    block_on(async {
        let mut app = TestApp::new(counter_app());
        app.pump_until_idle().await;
        expect(app.locator("#count"))
            .to_have_text("0")
            .await
            .unwrap();
        app.locator("#inc").click().await.unwrap();
        expect(app.locator("#count"))
            .to_have_text("1")
            .await
            .unwrap();
        app.locator("#inc").click().await.unwrap();
        expect(app.locator("#count"))
            .to_have_text("2")
            .await
            .unwrap();
    });
}

#[test]
fn screenshot_golden_round_trip() {
    block_on(async {
        let mut app = TestApp::new(counter_app());
        app.pump_until_idle().await;
        app.expect_screenshot("harness_counter").await;
    });
}
