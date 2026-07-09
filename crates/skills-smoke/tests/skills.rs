//! One test per skill, each pinning that skill's load-bearing snippet or
//! claim table to the real API. A failure here means a skill (and possibly
//! a spec section) must be updated in the same commit — see AGENT.md.

use kurbo::Size;
use lumen_core::events::{Event, PointerEvent};
use lumen_core::state::Signal;
use lumen_test::{block_on, expect, TestApp};
use lumen_widgets::{center, col, row, widgets, App, BuildCx, Element};

/// `building-apps` §2 — the composition snippet, verbatim.
fn counter(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i64);
    col![
        widgets::text(format!("Count: {}", count.get(cx.runtime()))).id("readout"),
        row![
            widgets::button("−", move |rt| count.update(rt, |c| *c -= 1)).id("dec"),
            widgets::button("+", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        ],
    ]
}

/// `building-apps` §2 + `verifying-apps` rung 2: TestApp + locator + expect.
#[test]
fn building_apps_composition_drives_and_asserts() {
    block_on(async {
        let mut app = TestApp::new(App::new(counter));
        app.pump_until_idle().await;
        app.locator("#inc").click().await.unwrap();
        expect(app.locator("#readout"))
            .to_have_text("Count: 1")
            .await
            .unwrap();
    });
}

/// `writing-widgets` Step 5 — the headless widget-test pattern: inject a
/// click, assert state + bounds + coherence.
#[test]
fn writing_widgets_headless_pattern() {
    let mut h = App::new(|cx| widgets::checkbox(cx, "t", "Label").id("box"))
        .run_headless(Size::new(200.0, 80.0));
    h.pump();
    let bounds = h.node_bounds_by_id("box").expect("checkbox laid out");
    let p = center(bounds);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    let on: Signal<bool> = h.runtime().signal("t", || false);
    assert!(on.get(h.runtime()), "click toggled the signal");
    h.assert_view_coherent();
}

/// `styling-lss` "works" table — background color, border-radius via
/// tokens/themes, and the canonical `get_styles` serialization
/// (`#rrggbbaa` colors, `{px: …}` lengths) the skill tells agents to
/// confirm rules with.
#[test]
fn styling_lss_working_subset() {
    let lss = r#"
@tokens { radius: 6px; }
@theme light { primary: #3b82f6ff; }
button.primary { background: $primary; border-radius: $radius; }
"#;
    let mut h = App::new(|_cx| widgets::button("Save", |_| {}).class("primary").id("save"))
        .stylesheet(lss)
        .run_headless(Size::new(200.0, 80.0));
    h.pump();
    let styles = h.get_styles("#save");
    assert_eq!(
        styles["background"]["value"],
        serde_json::json!("#3b82f6ff"),
        "token-resolved background missing; got {styles}"
    );
    assert_eq!(
        styles["border-radius"]["value"]["px"],
        serde_json::json!(6.0),
        "px-canonical radius missing; got {styles}"
    );
}

/// `verifying-apps` / `debugging-lumen` — headless locator failures are
/// structured (Timeout/NotFound with candidates), which the skills tell
/// agents to rely on for diagnosis.
#[test]
fn verifying_apps_structured_locator_errors() {
    block_on(async {
        let mut app = TestApp::new(App::new(counter));
        app.pump_until_idle().await;
        let err = app.locator("#does-not-exist").click().await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("Timeout") || msg.contains("NotFound"),
            "expected a structured miss, got: {msg}"
        );
    });
}

/// `verifying-apps` rung 4 + S0.1 — the shared agent client stays sound:
/// it compiles, and its pure tree helpers (flatten/find) behave as the
/// skills document. Skips silently where python3 is unavailable.
#[test]
fn agent_client_script_is_sound() {
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts");
    let compile = std::process::Command::new("python3")
        .args(["-m", "py_compile", &format!("{scripts}/agent_client.py")])
        .output();
    let Ok(compile) = compile else {
        eprintln!("python3 not available; skipping agent_client smoke");
        return;
    };
    assert!(
        compile.status.success(),
        "agent_client.py failed to compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let helpers = r#"
import sys
sys.path.insert(0, sys.argv[1])
from agent_client import AgentClient
tree = {"node": "node-0", "role": "group", "children": [
    {"node": "node-1", "role": "button", "id": "save", "label": "Save",
     "children": [], "states": [], "bounds": {}}], "states": [], "bounds": {}}
assert len(AgentClient.flatten(tree)) == 2
assert AgentClient.find(tree, id="save")["role"] == "button"
assert AgentClient.find(tree, label_contains="sav") is not None
assert AgentClient.find(tree, role="slider") is None
print("ok")
"#;
    let run = std::process::Command::new("python3")
        .args(["-c", helpers, scripts])
        .output()
        .expect("python3 ran above");
    assert!(
        run.status.success(),
        "agent_client helpers regressed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}
