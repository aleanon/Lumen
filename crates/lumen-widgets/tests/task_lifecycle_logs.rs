//! O4.4/O4.5: a spinner that spins forever explains itself.
//!
//! `finish()` stores `Err(e)` on the resource cell for the view to render. Early
//! in development a view usually does not render errors — so a failed fetch was
//! invisible, and indistinguishable from one still in flight. And
//! `drain_deferred()` returned a count that `pump` discarded, so "your data
//! arrived on frame N" — the line separating "the fetch never completed" from
//! "it completed and the view ignored it" — did not exist.

use kurbo::Size;
use lumen_widgets::{widgets, App, BuildCx, Element, TaskError};

fn logs(
    h: &lumen_widgets::Headless<lumen_render::TinySkia, lumen_core::tasks::ManualSpawner>,
) -> Vec<String> {
    h.runtime()
        .logs_since(0)
        .into_iter()
        .map(|e| format!("{}: {}", e.level, e.message))
        .collect()
}

/// A fetch that fails while the view shows no error UI at all — the common
/// early-development shape.
#[test]
fn a_failed_resource_is_reported_even_with_no_error_ui() {
    let spawner = lumen_core::tasks::ManualSpawner::new();
    let runner = spawner.clone();
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let r = cx.resource::<String, TaskError, _, _>("thing", (), |_| async {
            Err(TaskError::msg("connection refused"))
        });
        // Deliberately renders neither the error nor the loading state.
        let _ = r.is_ready();
        widgets::column(vec![widgets::text("static").id("t")]).id("root")
    })
    .with_executor(spawner)
    .run_headless(Size::new(300.0, 200.0));

    h.pump();
    runner.run_pending();
    h.pump();

    let found: Vec<String> = logs(&h)
        .into_iter()
        .filter(|l| l.contains("resource fetch failed"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "the failure must reach the ring even though nothing renders it: {:?}",
        logs(&h)
    );
    assert!(found[0].starts_with("warn:"), "at warn level: {}", found[0]);
}

/// The other half: a result that lands is announced, so "never completed" and
/// "completed and ignored" stop looking identical.
#[test]
fn an_applied_background_result_is_announced() {
    let spawner = lumen_core::tasks::ManualSpawner::new();
    let runner = spawner.clone();
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        let r = cx
            .resource::<String, TaskError, _, _>("thing", (), |_| async { Ok("done".to_string()) });
        let label = r.value.clone().unwrap_or_else(|| "…".into());
        widgets::column(vec![widgets::text(label).id("t")]).id("root")
    })
    .with_executor(spawner)
    .run_headless(Size::new(300.0, 200.0));

    h.pump();
    assert!(
        !logs(&h).iter().any(|l| l.contains("background result")),
        "nothing has landed yet"
    );

    runner.run_pending();
    h.pump();

    assert!(
        logs(&h).iter().any(|l| l.contains("background result")),
        "the arrival must be visible: {:?}",
        logs(&h)
    );
}
