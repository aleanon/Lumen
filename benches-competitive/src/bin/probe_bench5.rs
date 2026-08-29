//! BENCH5 / Lumen — see `harnesses/bench5/common.md` for the shared contract.
//!
//! Usage: `probe_bench5 <n> <iters> <point|churn> [rebuild|patch]`
//!
//! Two workloads, because they answer opposite questions:
//!
//! * `point` — one row's text changes in an N-row tree. This is the case
//!   Lumen's retained pipeline exists for, and the number should barely move
//!   with N.
//! * `churn` — every row's text changes. Nothing is reusable: no scope memo
//!   hits, no shaped-text cache hits, no spliced spans. This is the raw
//!   throughput floor, and it is the case Lumen's caches are least able to
//!   help with.
//!
//! Two paths, because Lumen has two and they differ by an order of magnitude:
//!
//! * `rebuild` — the signal is read inside the view, so the change is
//!   structural and the frame is rebuilt (memoized per row via `scope_with_deps`).
//! * `patch`   — the text is a `bind!` binding, so a change that measures the
//!   same size patches the node in place with no rebuild or relayout.
//!
//! The renderer is a null one: Lumen stops at the display list, matching where
//! the Qt harness's `set_layout` row stops and where GTK's `measure` row stops.
//! Rasterisation is measured by nobody here.
//!
//! `FrameStats` is asserted rather than trusted: a `churn` run that reported
//! `nodes_rebuilt == 0` would be measuring a memo hit, not churn, and a `patch`
//! run that rebuilt would be silently measuring the rebuild path. Both have
//! happened to probes in this directory before, and both look like a plausible
//! number rather than a failure.
use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App};
use std::time::Instant;

const VIEW_W: f64 = 400.0;
const VIEW_H: f64 = 800.0;

struct NullRenderer;
impl lumen_render::Renderer for NullRenderer {
    fn render_frame(
        &mut self, _l: &lumen_render::DisplayList, _w: u32, _h: u32, _s: f64,
        _b: lumen_core::Color,
    ) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(0, 0, Vec::new())
    }
    fn name(&self) -> &'static str { "null" }
}

fn proc_kb(key: &str) -> i64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find(|l| l.starts_with(key)).and_then(|l| {
                l.split_whitespace().nth(1).and_then(|v| v.parse().ok())
            })
        })
        .unwrap_or(-1)
}
fn rss() -> i64 { proc_kb("VmRSS:") }
fn hwm() -> i64 { proc_kb("VmHWM:") }

fn main() {
    let mut a = std::env::args().skip(1);
    let n: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(3000);
    let iters: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(200);
    let mode = a.next().unwrap_or_else(|| "point".into());
    let path = a.next().unwrap_or_else(|| "rebuild".into());
    let churn = mode == "churn";
    let patch = path == "patch";

    let rss_base = rss();

    // Both paths share the same content and the same driving signal, so the
    // only difference between them is HOW the new string reaches the node.
    let mut h = if patch {
        App::new(move |cx| {
            let _ = cx.signal("n", || 0i64);
            let rows: Vec<_> = (0..n)
                .map(|i| {
                    let t = widgets::text(format!("row {i:04} 00000"));
                    if churn || i == 0 {
                        t.bind_text(bind!(rt => {
                            let s: Signal<i64> = rt.signal("n", || 0i64);
                            format!("row {i:04} {:05}", s.get(rt) % 100000)
                        }))
                    } else {
                        t
                    }
                })
                .collect();
            widgets::column(rows)
        })
        .with_renderer(NullRenderer)
        .run_headless(Size::new(VIEW_W, VIEW_H))
    } else {
        App::new(move |cx| {
            let bump = cx.signal("n", || 0i64).get(cx.runtime());
            let rows: Vec<_> = (0..n)
                .map(|i| {
                    // `churn` makes every row depend on the counter, so no
                    // scope memo can hit; `point` makes only row 0 depend on
                    // it, so N-1 spans splice forward.
                    let dep = if churn || i == 0 { bump } else { 0 };
                    cx.scope_with_deps(("row", i), dep, move |_cx| {
                        widgets::text(format!("row {i:04} {:05}", dep % 100000))
                    })
                })
                .collect();
            widgets::column(rows)
        })
        .with_renderer(NullRenderer)
        .run_headless(Size::new(VIEW_W, VIEW_H))
    };

    h.pump();
    let sig: Signal<i64> = h.runtime().signal("n", || 0);
    for _ in 0..20 {
        sig.update(h.runtime(), |v| *v += 1);
        h.pump();
    }
    let rss_built = rss();

    // One instrumented frame BEFORE the timing loop, so what the loop measures
    // is on the record rather than assumed.
    sig.update(h.runtime(), |v| *v += 1);
    let st = h.pump();

    println!("BENCH5\tfw=lumen-{path}\tmode={mode}\tn={n}\titers={iters}");
    println!("nodes.total\t{}", st.node_count);
    println!("nodes.rebuilt\t{}", st.nodes_rebuilt);
    println!("nodes.copied\t{}", st.nodes_copied);
    println!("painted\t{}", st.painted);

    // The workload must actually be the workload. A silent memo hit here would
    // read as a spectacular result.
    if patch {
        assert_eq!(
            st.nodes_rebuilt, 0,
            "patch path rebuilt {} nodes - this is measuring the rebuild path",
            st.nodes_rebuilt
        );
    } else if churn {
        assert!(
            st.nodes_rebuilt as usize >= n,
            "churn rebuilt only {} of {n} rows - the memo is hitting, so this \
             is not churn",
            st.nodes_rebuilt
        );
    } else {
        assert!(
            st.nodes_copied > 0,
            "point-mode rebuild copied no spans forward - memoization is off, \
             so this is measuring a full rebuild"
        );
    }
    assert!(st.painted, "the frame did not paint - nothing was measured");

    let mut best = f64::MAX;
    for _ in 0..iters {
        sig.update(h.runtime(), |v| *v += 1);
        let t = Instant::now();
        std::hint::black_box(h.pump());
        best = best.min(t.elapsed().as_secs_f64() * 1e6);
    }
    println!("total_us\t{best:.1}");
    println!("rss.base_kb\t{rss_base}");
    println!("rss.built_kb\t{rss_built}");
    println!("rss.peak_kb\t{}", hwm());
}
