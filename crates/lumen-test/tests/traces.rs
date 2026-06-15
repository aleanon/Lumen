//! T2.2 acceptance: traces validate against the schema, and a failure embeds
//! the screenshot + tree.

use lumen_test::{block_on, TestApp};
use lumen_widgets::{widgets, App};
use serde_json::Value;

fn counter() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("Count: {v}")).id("count"),
            widgets::button("+1", move |rt| count.update(rt, |c| *c += 1)).id("increment"),
        ])
    })
}

#[test]
fn trace_records_and_validates_against_schema() {
    block_on(async {
        let app = TestApp::new(counter());
        app.trace_action("click", "#increment");
        app.locator("#increment").click().await.unwrap();
        app.trace_assert("to_have_text(Count: 1)", true);

        let path = app.write_trace("trace_demo");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.is_empty());

        let schema: Value = serde_json::from_str(include_str!("../schema/trace-1.json")).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for line in content.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            assert!(validator.is_valid(&v), "schema-invalid trace line: {line}");
        }

        let types: Vec<String> = app
            .trace_events()
            .iter()
            .map(|e| e["type"].as_str().unwrap().to_string())
            .collect();
        assert!(types.contains(&"action".to_string()));
        assert!(types.contains(&"tree".to_string()));
        assert!(types.contains(&"assert".to_string()));
    });
}

#[test]
fn failure_embeds_screenshot_and_tree() {
    block_on(async {
        let app = TestApp::new(counter());
        app.locator("#increment").click().await.unwrap();
        let path = app.capture_failure("trace_failure", "expected Count: 5, got Count: 1");

        let content = std::fs::read_to_string(&path).unwrap();
        let last: Value = serde_json::from_str(content.lines().last().unwrap()).unwrap();
        assert_eq!(last["type"], "failure");
        assert!(
            last["screenshot_base64"].as_str().unwrap().len() > 100,
            "screenshot not embedded"
        );
        assert!(last["tree"]["root"].is_object(), "tree not embedded");
    });
}
