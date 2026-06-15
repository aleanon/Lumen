//! Session recording + export (05 §session, T2.5).
//!
//! A [`Session`] drives a [`TestApp`] exactly like a hand-written test, but
//! records every step. [`Session::export_test`] then emits an equivalent,
//! standalone `lumen-test` integration test that replays the session — the
//! "record once, regress forever" workflow the agent uses to turn exploration
//! into a committed regression suite.

use crate::{expect, LocatorError, TestApp};
use std::fmt::Write as _;

enum Step {
    Click(String),
    Fill(String, String),
    Press(String, String),
    ExpectText(String, String),
    ExpectValue(String, String),
    ExpectState(String, String),
}

/// Records interactions against a [`TestApp`] and exports them as a test.
pub struct Session {
    app: TestApp,
    app_expr: String,
    steps: Vec<Step>,
}

impl Session {
    /// Wrap `app`. `app_expr` is the Rust expression that reconstructs the app
    /// in exported code (e.g. `"counter()"`).
    pub fn new(app: TestApp, app_expr: &str) -> Session {
        Session {
            app,
            app_expr: app_expr.to_string(),
            steps: Vec::new(),
        }
    }

    /// The wrapped app, for screenshots or ad-hoc assertions.
    pub fn app(&self) -> &TestApp {
        &self.app
    }

    /// Click the node matched by `selector` (recorded).
    pub async fn click(&mut self, selector: &str) -> Result<(), LocatorError> {
        self.app.locator(selector).click().await?;
        self.steps.push(Step::Click(selector.into()));
        Ok(())
    }

    /// Fill the node matched by `selector` with `text` (recorded).
    pub async fn fill(&mut self, selector: &str, text: &str) -> Result<(), LocatorError> {
        self.app.locator(selector).fill(text).await?;
        self.steps.push(Step::Fill(selector.into(), text.into()));
        Ok(())
    }

    /// Press `key` on the node matched by `selector` (recorded).
    pub async fn press(&mut self, selector: &str, key: &str) -> Result<(), LocatorError> {
        self.app.locator(selector).press(key).await?;
        self.steps.push(Step::Press(selector.into(), key.into()));
        Ok(())
    }

    /// Assert the node's text equals `text` (recorded).
    pub async fn expect_text(&mut self, selector: &str, text: &str) -> Result<(), LocatorError> {
        expect(self.app.locator(selector))
            .to_have_text(text)
            .await?;
        self.steps
            .push(Step::ExpectText(selector.into(), text.into()));
        Ok(())
    }

    /// Assert the node's value equals `value` (recorded).
    pub async fn expect_value(&mut self, selector: &str, value: &str) -> Result<(), LocatorError> {
        expect(self.app.locator(selector))
            .to_have_value(value)
            .await?;
        self.steps
            .push(Step::ExpectValue(selector.into(), value.into()));
        Ok(())
    }

    /// Assert the node carries state `state` (e.g. `"checked"`) (recorded).
    pub async fn expect_state(&mut self, selector: &str, state: &str) -> Result<(), LocatorError> {
        expect(self.app.locator(selector))
            .to_have_state(state)
            .await?;
        self.steps
            .push(Step::ExpectState(selector.into(), state.into()));
        Ok(())
    }

    /// Number of recorded steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Export a standalone `#[test]` that replays the session. `header` is
    /// prepended verbatim (imports + the app-builder fn named by the `app_expr`
    /// passed to [`Session::new`]). The result is valid, `cargo test`-able Rust.
    pub fn export_test(&self, fn_name: &str, header: &str) -> String {
        // `{:?}` on a &str yields a correctly-escaped Rust string literal, so
        // selectors/text with quotes or backslashes round-trip safely.
        let mut s = String::new();
        let _ = writeln!(s, "{header}");
        let _ = writeln!(s, "#[test]");
        let _ = writeln!(s, "fn {fn_name}() {{");
        let _ = writeln!(s, "    lumen_test::block_on(async {{");
        let _ = writeln!(
            s,
            "        let app = lumen_test::TestApp::new({});",
            self.app_expr
        );
        for step in &self.steps {
            let line = match step {
                Step::Click(sel) => {
                    format!("        app.locator({sel:?}).click().await.unwrap();")
                }
                Step::Fill(sel, t) => {
                    format!("        app.locator({sel:?}).fill({t:?}).await.unwrap();")
                }
                Step::Press(sel, k) => {
                    format!("        app.locator({sel:?}).press({k:?}).await.unwrap();")
                }
                Step::ExpectText(sel, t) => format!(
                    "        lumen_test::expect(app.locator({sel:?})).to_have_text({t:?}).await.unwrap();"
                ),
                Step::ExpectValue(sel, v) => format!(
                    "        lumen_test::expect(app.locator({sel:?})).to_have_value({v:?}).await.unwrap();"
                ),
                Step::ExpectState(sel, st) => format!(
                    "        lumen_test::expect(app.locator({sel:?})).to_have_state({st:?}).await.unwrap();"
                ),
            };
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s, "    }});");
        let _ = writeln!(s, "}}");
        s
    }
}
