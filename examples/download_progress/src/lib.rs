//! download_progress — M.5 (ADR-M2): streaming progress through the `Sink`.
//! A background job (here a simulated transfer; swap in your client's
//! read loop) pushes progress into a signal chunk by chunk — the UI renders
//! each update as it lands.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};

const TOTAL: u64 = 100;

/// Build the download app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let progress = cx.signal("progress", || 0u64);
    let started = cx.signal("started", || false);
    let p = progress.get(cx.runtime());

    if started.get(cx.runtime()) {
        // One background job per start; chunks stream back through the Sink.
        cx.task_blocking("download", started.get(cx.runtime()), move |_, sink| {
            for done in 1..=TOTAL {
                // A real client: read a chunk here (ureq reader / reqwest
                // stream), then report. The Sink is the ONLY handle that
                // crosses back — each set applies on the next pump.
                sink.set(progress, done);
            }
        });
    }

    let mut col = widgets::column(vec![
        widgets::text("Download with progress").id("title"),
        widgets::progress_bar((p as f64) / (TOTAL as f64)).id("bar"),
        widgets::text(format!("{p}%")).id("pct"),
        widgets::button("Start", move |rt| started.set(rt, true)).id("start"),
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
