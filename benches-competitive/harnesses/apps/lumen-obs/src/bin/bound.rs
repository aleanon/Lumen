//! A11Y3: the cost of the agent-only dep bookkeeping on the *build* path.
//!
//! `dep_keys()` builds a `Vec<String>` per **bound** node so `ui.getDeps` can
//! name the signals. Reactivity itself runs off the `ReadSet`, not these
//! strings. The earlier fwbench arms showed no change because their labels are
//! plain `format!` strings — no binding, so the code never ran. This workload
//! makes every row a bound node, which is where the work actually is.
use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App, BuildCx, Element};

fn main() {
    let n: usize = std::env::var("ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(5_000);
    let mut h = App::new(move |cx: &mut BuildCx| {
        let kids: Vec<Element> = (0..n)
            .map(|i| {
                widgets::text(bind!(rt => {
                    let c: Signal<i64> = rt.signal("tick", || 0i64);
                    format!("row {i} · {}", c.get(rt))
                }))
            })
            .collect();
        let _: Signal<i64> = cx.signal("tick", || 0);
        widgets::column(kids)
    })
    .run_headless(Size::new(400.0, 600.0));
    h.pump();

    let tick: Signal<i64> = h.runtime().signal("tick", || 0);
    // Force a full rebuild each frame by moving the signal every row reads.
    for i in 0..10 {
        tick.set(h.runtime(), i);
        h.pump();
    }
    let mut best = u128::MAX;
    for i in 10..50 {
        tick.set(h.runtime(), i);
        let t = std::time::Instant::now();
        h.pump();
        best = best.min(t.elapsed().as_micros());
    }
    println!("bound\tobs={}\tN={n}\tframe_us={best}", cfg!(feature = "obs"));
}
