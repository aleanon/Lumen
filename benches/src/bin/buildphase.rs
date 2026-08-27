//! What does a CHANGED frame cost, and how much of it is lowering?
//!
//! `animmemo` measures a memo-hitting frame — `rebuilt=2` — so it cannot say
//! anything about the cost of lowering nodes. This is the opposite shape: every
//! node is rebuilt every frame, which is what direct lowering would actually
//! change.

use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, Element};
use std::time::Instant;

fn rows() -> i64 {
    std::env::var("ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000)
}

/// `churn` = every row's string is new every frame (100% shape-cache miss).
/// `stable` = every row re-lowers with the SAME string, so shaping is cached
/// and what remains is the lowering work itself. Real frames sit between them;
/// `stable` is the closer analogue of "a filter/hover re-ran the view".
fn mode() -> String {
    std::env::var("MODE").unwrap_or_else(|_| "churn".into())
}

/// Flat rows all reading one root signal, so a write re-runs the whole closure
/// and every node is lowered fresh. The `nodecost.rs` `flat_app` shape.
/// A small but realistic sheet: type rules, a class rule, a state rule and a
/// descendant selector, so the cascade does real matching work per node.
const SHEET: &str = "
column { padding: 4px; }
text { color: #202020; font-size: 14px; }
.row { padding: 2px; }
.row:hover { color: #0055cc; }
column .row { margin: 1px; }
";

/// Nesting depth wrapped around each row. Real views are not flat lists of
/// leaves — a row sits in a card in a section in a panel — and
/// `span_ctx_hash` is O(depth) per node, so depth is the axis it lives on.
fn depth() -> usize {
    std::env::var("DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn app() -> App {
    let churn = mode() == "churn";
    let sheet = std::env::var("SHEET").is_ok();
    let d = depth();
    let defsize = std::env::var("DEFSIZE").is_ok();
    let ids = std::env::var("IDS").is_ok();
    let a = App::new(move |cx| {
        let bump = cx.signal("n", || 0i64).get(cx.runtime());
        let n = rows();
        let rows: Vec<_> = (0..n)
            .map(|i| {
                let t = if churn {
                    widgets::text(format!("row {i} · {bump}"))
                } else {
                    widgets::text(format!("row {i}"))
                };
                let mut e: Element = t.class("row");
                // IDS=1: a unique id per row, so every node has a distinct
                // style identity and the A.5b memo misses on all of them —
                // the shape a real keyed list has.
                if ids {
                    e = e.id(format!("r{i}"));
                }
                for k in 0..d {
                    let mut w: Element = widgets::column(vec![e]).class(if k % 2 == 0 {
                        "wrap-a"
                    } else {
                        "wrap-b"
                    });
                    // DEFSIZE=1: give every wrapper a definite box, so no level
                    // needs an intrinsic-size pass over its children.
                    if defsize {
                        w.style.width = lumen_layout::Dim::px(300.0);
                        w.style.height = lumen_layout::Dim::px(20.0);
                    }
                    e = w;
                }
                e
            })
            .collect();
        widgets::column(rows)
    });
    if sheet {
        a.stylesheet(SHEET)
    } else {
        a
    }
}

fn main() {
    let mut h = app().run_headless(Size::new(400.0, 600.0));
    for _ in 0..5 {
        h.pump();
    }
    let bump = |h: &mut lumen_widgets::Headless| {
        let s: Signal<i64> = h.runtime().signal("n", || 0);
        s.update(h.runtime(), |v| *v += 1);
        h.pump()
    };
    for _ in 0..10 {
        bump(&mut h);
    }
    let mut us = Vec::new();
    let mut last = None;
    for _ in 0..40 {
        let t = Instant::now();
        let st = bump(&mut h);
        us.push(t.elapsed().as_secs_f64() * 1e6);
        last = Some(st);
    }
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let st = last.unwrap();
    println!(
        "changed-frame[{}{} d{}]\t{:.1}\t{:.1}\tnodes={} rebuilt={} copied={}",
        mode(),
        if std::env::var("SHEET").is_ok() {
            "+sheet"
        } else {
            ""
        },
        depth(),
        us[0],
        us[us.len() / 2],
        st.node_count,
        st.nodes_rebuilt,
        st.nodes_copied
    );
}
