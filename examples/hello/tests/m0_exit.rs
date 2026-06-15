//! M0-exit: drive the hello counter headlessly — query the tree, click
//! `#increment` by selector, assert the label, and match an exact golden.
//! Runs in CI on Linux/Windows/macOS.

use lumen_test::{block_on, expect, TestApp};

#[test]
fn counter_increments_and_matches_golden() {
    block_on(async {
        let mut app = TestApp::new(hello::main_app());
        app.pump_until_idle().await;

        // The label starts at zero.
        expect(app.locator("#count"))
            .to_have_text("Count: 0")
            .await
            .unwrap();

        // Click the increment button by selector; the label updates.
        app.locator("#increment").click().await.unwrap();
        expect(app.locator("#count"))
            .to_have_text("Count: 1")
            .await
            .unwrap();

        // Exact golden of the rendered frame.
        app.expect_screenshot("m0_exit_counter").await;
    });
}
