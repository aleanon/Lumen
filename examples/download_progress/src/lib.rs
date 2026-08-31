//! download_progress — M.5 (ADR-M2): streaming progress through the `Sink`,
//! and TC1: stopping the transfer on demand.
//!
//! A background job (here a simulated transfer; swap in your client's read
//! loop) pushes progress into a signal chunk by chunk — the UI renders each
//! update as it lands. The job polls `sink.is_cancelled()` between chunks, so
//! Cancel stops it mid-flight instead of merely ignoring the rest.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, AbortHandle, App, BuildCx, Element};

const TOTAL: u64 = 100;
/// Paced so the transfer is long enough to actually catch with Cancel.
const CHUNK_MS: u64 = 8;

/// Build the download app.
pub fn main_app() -> App {
    // E2b: `App::view` root. The body keeps its `Element` root for now — the
    // page centers itself with `align_items`/`justify_content`, which `Stack`
    // does not express yet (recorded as an E2 API gap).
    App::view(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let progress = cx.signal("progress", || 0u64);
    // The *run counter*, not a bool: it doubles as the task's deps, so each
    // Start supersedes the previous generation and gets a fresh task. A plain
    // `started: bool` could not restart after a cancel — same deps, same task.
    let run = cx.signal("run", || 0u64);
    let r = run.get(cx.runtime());
    let p = progress.get(cx.runtime());

    let mut cancel: Option<AbortHandle> = None;
    if r > 0 {
        cancel = Some(cx.abortable_task_blocking("download", r, move |_, sink| {
            for done in 1..=TOTAL {
                // Cancellation is already *correct* without this check — no
                // write of a cancelled task lands. Checking is what makes it
                // *prompt*: otherwise the thread keeps transferring for nothing.
                if sink.is_cancelled() {
                    break;
                }
                // A real client: read a chunk here (ureq reader / reqwest
                // stream), then report. The Sink is the ONLY handle that
                // crosses back — each set applies on the next pump.
                #[cfg(not(target_arch = "wasm32"))]
                std::thread::sleep(std::time::Duration::from_millis(CHUNK_MS));
                sink.set(progress, done);
            }
        }));
    }
    let aborted = cancel.as_ref().is_some_and(|h| h.is_aborted(cx.runtime()));

    let status = match (r, p, aborted) {
        (0, _, _) => "idle".to_string(),
        (_, _, true) => format!("cancelled at {p}%"),
        (_, TOTAL, _) => "done".to_string(),
        _ => format!("{p}%"),
    };

    let mut actions = widgets::row(vec![
        widgets::button("Start", move |rt| {
            progress.set(rt, 0);
            run.update(rt, |v| *v += 1);
        })
        .id("start"),
        widgets::button("Cancel", move |rt| {
            if let Some(h) = &cancel {
                h.abort(rt);
            }
        })
        .id("cancel"),
    ])
    .id("actions");
    actions.style.column_gap = Dim::px(8.0);

    let mut col = widgets::column(vec![
        widgets::text("Download with progress").id("title"),
        widgets::progress_bar((p as f64) / (TOTAL as f64)).id("bar"),
        widgets::text(status).id("pct"),
        actions,
    ])
    .id("page");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(12.0),
        ..LayoutStyle::default()
    };
    col
}
