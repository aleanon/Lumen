//! F4.5 conformance: the reified-graph verbs route through `dispatch` and agree.
//! `whatDependsOn` predicts, `lastChange` confirms, `getDeps` breaks down per
//! prop, and `invokeAction` actuates geometry-free.

use lumen_agent::dispatch;
use lumen_core::geometry::Size;
use lumen_widgets::{text, widgets, App, BuildCx};
use serde_json::json;

fn call(
    app: &mut lumen_widgets::Headless,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    dispatch(app, &req)["result"].clone()
}

#[test]
fn f4_verbs_dispatch_and_agree() {
    let mut app = App::new(|cx: &mut BuildCx| {
        let n: lumen_core::Signal<i64> = cx.signal("n", || 0);
        widgets::column(vec![
            text!(cx, "n={n}").id("lbl"),
            widgets::button("+1", move |rt| n.update(rt, |c| *c += 1)).id("inc"),
        ])
    })
    .run_headless(Size::new(200.0, 120.0));

    // getDeps: the label's text prop depends on `n`.
    let deps = call(&mut app, "ui.getDeps", json!({ "selector": "#lbl" }));
    assert_eq!(deps["byProp"]["text"], json!(["n"]));

    // whatDependsOn(n): the label, via text, updates by rebuild.
    let wdo = call(&mut app, "ui.whatDependsOn", json!({ "signal": "n" }));
    assert_eq!(wdo["dependents"][0]["via"], "text");
    assert_eq!(wdo["dependents"][0]["update"], "rebuild");

    // invokeAction: click the button by its handler (geometry-free).
    let act = call(
        &mut app,
        "input.invokeAction",
        json!({ "selector": "#inc" }),
    );
    assert_eq!(act["ok"], json!(true));

    // lastChange: that pump was a rebuild (the text binding is structural).
    let lc = call(&mut app, "ui.lastChange", json!({}));
    assert_eq!(lc["kind"], "rebuild");

    // And the tree reflects the click.
    let tree = call(&mut app, "ui.getTree", json!({}));
    assert!(tree.to_string().contains("n=1"));
}
