//! T7.5: the agent autonomously repairs an injected regression via structured
//! diagnostics — detect (W0103 overflow) → fix (#fix) → verify, zero human edits.
use lumen_agent::{auto_repair, dispatch};
use lumen_core::geometry::Size;
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};
use serde_json::json;

// An app with an injected layout overflow, fixable by clicking #fix.
fn buggy(cx: &mut BuildCx) -> Element {
    let bug = cx.signal("bug", || true);
    let on = bug.get(cx.runtime());
    let mut kids = vec![widgets::button("Fix", move |rt| bug.set(rt, false)).id("fix")];
    if on {
        kids.push(
            Element {
                role: lumen_core::semantics::Role::Group,
                style: lumen_layout::LayoutStyle {
                    width: lumen_layout::Dim::px(40.0),
                    height: lumen_layout::Dim::px(16.0),
                    ..Default::default()
                },
                children: vec![Element {
                    role: lumen_core::semantics::Role::Text,
                    label: "overflow".into(),
                    style: lumen_layout::LayoutStyle {
                        min_width: lumen_layout::Dim::px(200.0),
                        ..Default::default()
                    },
                    ..Element::default()
                }
                .id("bug-child")],
                ..Element::default()
            }
            .id("bug-box"),
        );
    }
    widgets::column(kids)
}

#[test]
fn agent_auto_repairs_layout_regression() {
    let mut app = App::new(buggy).run_headless(Size::new(300.0, 120.0));
    app.pump();
    assert!(!app.diagnostics().is_empty(), "regression present (W0103)");

    // The fixer knows: a W0103 overflow is fixed by invoking #fix.
    let rounds = auto_repair(&mut app, 5, |a: &mut Headless, d| {
        if d.code == "W0103" {
            dispatch(
                a,
                &json!({ "id": 1, "method": "input.click", "params": { "selector": "#fix" } }),
            );
            true
        } else {
            false
        }
    });

    assert!(rounds <= 2, "repaired in {rounds} round(s)");
    assert!(
        app.diagnostics().is_empty(),
        "app self-healed, zero human edits"
    );
}
