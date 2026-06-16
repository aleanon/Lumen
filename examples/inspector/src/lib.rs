//! The Lumen **Inspector** (T4.4) — built in Lumen itself. Four panels behind a
//! tab bar: a semantic **tree view**, a **style editor**, an **animation
//! scrubber**, and a **trace replay**. Because it is an ordinary Lumen app it is
//! driveable through `lumen-agent` like any other (see `tests/self_drive.rs`).

use lumen_widgets::widgets_m4::TreeRow;
use lumen_widgets::{widgets, widgets_m1, widgets_m4, App, BuildCx, Element};

/// A fixed sample trace the replay panel steps through.
const TRACE: &[&str] = &[
    "action click #increment",
    "tree snapshot (3 nodes)",
    "assert to_have_text Count: 1 -> pass",
];

/// Build the inspector application.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let tab = cx.signal("tab", || 0usize);
    let current = tab.get(cx.runtime());

    let header = widgets::row(vec![
        widgets::text("Lumen Inspector").id("title"),
        widgets_m1::spacer(),
    ]);
    let nav = widgets_m1::tabs(cx, "tab", &["Tree", "Style", "Scrub", "Trace"]);

    let panel = match current {
        0 => tree_panel(cx),
        1 => style_panel(cx),
        2 => scrub_panel(cx),
        _ => trace_panel(cx),
    };

    widgets::column(vec![header, nav, widgets_m1::divider(), panel]).id("root")
}

/// Tree view: the (sample) semantic tree of the inspected app.
fn tree_panel(cx: &BuildCx) -> Element {
    let rows = [
        TreeRow {
            id: "win",
            label: "window",
            depth: 0,
            has_children: true,
        },
        TreeRow {
            id: "col",
            label: "column",
            depth: 1,
            has_children: true,
        },
        TreeRow {
            id: "count",
            label: "text #count",
            depth: 2,
            has_children: false,
        },
        TreeRow {
            id: "inc",
            label: "button #increment",
            depth: 2,
            has_children: false,
        },
    ];
    widgets::column(vec![
        widgets::text("Semantic tree").id("panel-tree"),
        widgets_m4::tree(cx, "insp-tree", &rows),
    ])
}

/// Style editor: edit a numeric style property and preview it.
fn style_panel(cx: &BuildCx) -> Element {
    let size = cx.signal("font-size", || 16i64);
    let v = size.get(cx.runtime());
    widgets::column(vec![
        widgets::text("Style editor").id("panel-style"),
        widgets::row(vec![
            widgets::text("font-size").id("style-prop"),
            widgets_m1::stepper(cx, "font-size", 8, 72).id("font-size-stepper"),
        ]),
        widgets::text(format!("preview: {v}px")).id("style-preview"),
    ])
}

/// Animation scrubber: a slider over the timeline showing the current frame.
fn scrub_panel(cx: &BuildCx) -> Element {
    let t = cx.signal("scrub", || 0.0f64);
    let frame = (t.get(cx.runtime())).round() as i64;
    widgets::column(vec![
        widgets::text("Animation scrubber").id("panel-scrub"),
        widgets::slider(cx, "scrub", 0.0, 100.0).id("scrub-slider"),
        widgets::text(format!("frame {frame}")).id("scrub-frame"),
    ])
}

/// Trace replay: step through a recorded trace event by event.
fn trace_panel(cx: &BuildCx) -> Element {
    let step = cx.signal("trace-step", || 0usize);
    let i = step.get(cx.runtime()).min(TRACE.len() - 1);
    let next = widgets::button("Next", move |rt| {
        step.update(rt, |s| *s = (*s + 1).min(TRACE.len() - 1))
    })
    .id("trace-next");
    widgets::column(vec![
        widgets::text("Trace replay").id("panel-trace"),
        widgets::text(format!("[{}/{}] {}", i + 1, TRACE.len(), TRACE[i])).id("trace-event"),
        next,
    ])
}
