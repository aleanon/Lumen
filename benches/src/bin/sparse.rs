//! **The workload every real frame has and no other arm has**: N rows of which
//! only K change.
//!
//! `fwbench` changes *every* row's text every frame. That was deliberate — it
//! is the only way to make text shaping equal across five frameworks — but it
//! makes that benchmark structurally blind to the three things Lumen relies on
//! to be fast: scope memoization (F1), the per-property patch path (F3), and
//! taffy's per-node layout cache (R6). All three pay off exactly when *few*
//! nodes change, so measuring them on fwbench measures nothing.
//!
//! Reports `nodes_rebuilt` — the O(changed) meter — alongside frame time,
//! because "did it get faster" and "did it stop re-lowering the whole tree"
//! are different questions and only the second one diagnoses.
//!
//! ```text
//! ROWS=10000 CHANGED=10 DEPTH=0 MODE=plain|scope|bind|chunkbind cargo run --release --bin sparse
//! GROW=1 makes bound text grow one glyph per bump, so every patch DECLINES
//! (the box would widen) — the decline-cliff arm MUT1 exists for.
//! ```
//!
//! Modes, which exist to isolate *which* mechanism engages:
//!   plain  — the view reads every row's signal at top level. Structural, so a
//!            single write rebuilds everything. The control.
//!   scope  — each row in `cx.scope_with_deps`. Only invalidated scopes re-run;
//!            the rest splice in place (F1 + F2.2).
//!   bind   — each row's text is a `bind!` binding. Isolated reads, so the
//!            patch path (F3) should update the field without a rebuild.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App, BuildCx, Element, Label, Stack};
use std::time::Instant;

fn env(k: &str, d: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))?
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

/// Row `i`'s counter. One signal per row, so a write touches exactly one row's
/// dependency — the whole point of the workload.
///
/// An integer key, NOT `format!("r{i}")`. ADR-021 made identity `impl Hash +
/// Debug` precisely so per-row state costs no allocation ("re-addressing 1 000
/// per-row signals went 51.0 µs / 1 000 allocations → 18.2 µs / 0"). The first
/// version of this benchmark used the `format!` key it was written to kill,
/// which put 50 000 String allocations per frame into the measurement and made
/// the view closure look like the floor.
fn key(i: usize) -> usize {
    i
}

fn main() {
    let rows = env("ROWS", 10_000);
    let changed = env("CHANGED", 10).min(rows);
    let depth = env("DEPTH", 0);
    let win_h = env("WINH", 600) as f64;
    let mode = std::env::var("MODE").unwrap_or_else(|_| "plain".into());

    let m = mode.clone();
    let t0 = Instant::now();
    // MODE=direct (MUT7b): the statement-form Direct root — no Vec<Element>
    // staging, children lowered as they are written. Rows are bound labels,
    // so steady frames take the same patch path as MODE=bind; what this arm
    // isolates is the build/view side of the authoring form.
    let app = if m == "direct" {
        let fill = std::env::var("NOFILL").is_err();
        App::view(move |_cx: &mut BuildCx| {
            let s = Stack::column(move |c| {
                for i in 0..rows {
                    c.child(Label::new(bind!(rt => {
                        let v: Signal<i64> = rt.signal(key(i), || 0);
                        format!("row {i} · {}", v.get(rt))
                    })));
                }
            });
            if fill {
                s.width(lumen_layout::Dim::pct(1.0))
            } else {
                s
            }
        })
    } else {
        App::new(move |cx: &mut BuildCx| {
        // MODE=for: the materialized keyed collection. Same chunked memo as
        // `chunk`/`component`, but the widget picks the grain instead of the
        // author — the point of R10.
        if m == "for" {
            let vals: Vec<i64> = (0..rows)
                .map(|i| {
                    let v: Signal<i64> = cx.signal(key(i), || 0);
                    v.get(cx.runtime())
                })
                .collect();
            let mut root: Element = lumen_widgets::For::new(cx, "rows", &vals, |_cx, i, v| {
                let mut e: Element = widgets::text(format!("row {i} · {v}"));
                for _ in 0..depth {
                    e = widgets::column(vec![e]);
                }
                e
            })
            .into();
            if std::env::var("NOFILL").is_err() {
                root.style.width = lumen_layout::Dim::pct(1.0);
            }
            return root;
        }
        // MODE=virtual: the collection widget that owns its own granularity.
        // `VirtualList` calls `render(i)` only over the visible window, so the
        // view loop, the build, the layout and the paint are all O(visible)
        // rather than O(N) or O(N/chunk) — no author-chosen grain at all.
        if m == "virtual" {
            let rt = cx.runtime().clone();
            return lumen_widgets::VirtualList::new(
                cx,
                "vl",
                rows,
                21.0,
                win_h,
                move |i| {
                    let v: Signal<i64> = rt.signal(key(i), || 0);
                    widgets::text(format!("row {i} · {}", v.get(&rt)))
                },
            )
            .into();
        }
        // MODE=component: the same grouping as `chunk`, expressed as a
        // `Component`. This is the arm that proves the abstraction costs
        // nothing over the hand-written scope it packages — if it does not
        // match `chunk`, the trait is overhead rather than ergonomics.
        if m == "component" {
            // S2: `#[derive(Hash)]` and no `deps` — every field participates,
            // so it cannot omit one.
            #[derive(std::hash::Hash)]
            struct RowGroup {
                lo: usize,
                hi: usize,
                depth: usize,
                vals: Vec<i64>,
            }
            impl lumen_widgets::Component for RowGroup {
                fn build(&self, _cx: &mut BuildCx) -> Element {
                    let items: Vec<Element> = (self.lo..self.hi)
                        .map(|i| {
                            let mut e: Element =
                                widgets::text(format!("row {i} · {}", self.vals[i - self.lo]));
                            for _ in 0..self.depth {
                                e = widgets::column(vec![e]);
                            }
                            e
                        })
                        .collect();
                    widgets::column(items)
                }
            }
            let chunk = std::env::var("CHUNK")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(64)
                .max(1);
            let groups: Vec<Element> = (0..rows.div_ceil(chunk))
                .map(|g| {
                    let lo = g * chunk;
                    let hi = ((g + 1) * chunk).min(rows);
                    let vals: Vec<i64> = (lo..hi)
                        .map(|i| {
                            let v: Signal<i64> = cx.signal(key(i), || 0);
                            v.get(cx.runtime())
                        })
                        .collect();
                    cx.component(
                        ("group", g),
                        RowGroup {
                            lo,
                            hi,
                            depth,
                            vals,
                        },
                    )
                })
                .collect();
            let mut root: Element = widgets::column(groups);
            if std::env::var("NOFILL").is_err() {
                root.style.width = lumen_layout::Dim::pct(1.0);
            }
            return root;
        }
        // MODE=chunk: rows grouped `CHUNK` at a time under ONE scope. The
        // per-row view cost only disappears if the loop itself stops running,
        // which needs a scope ABOVE the loop, not inside it. This is the
        // standard escape from the O(N) view floor and the shape a keyed-list
        // construct (`For`, not yet built) would generate.
        if m == "chunk" {
            let chunk = std::env::var("CHUNK")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(64)
                .max(1);
            let groups: Vec<Element> = (0..rows.div_ceil(chunk))
                .map(|g| {
                    let lo = g * chunk;
                    let hi = ((g + 1) * chunk).min(rows);
                    // The chunk's dep is the sum of its rows' versions: cheap,
                    // and changes iff any row in the chunk changed.
                    // Read the values in the OUTER cx and move them in.
                    // `cx2.signal(i)` inside the scope would be SCOPE-LOCAL
                    // (F1 namespaces scope signals), so it would read a fresh
                    // always-zero signal rather than the one `bump` writes —
                    // caught by the equivalence guard, which is why it exists.
                    let vals: Vec<i64> = (lo..hi)
                        .map(|i| {
                            let v: Signal<i64> = cx.signal(key(i), || 0);
                            v.get(cx.runtime())
                        })
                        .collect();
                    let mut acc: i64 = 0;
                    for v in &vals {
                        acc = acc.wrapping_mul(31).wrapping_add(*v);
                    }
                    cx.scope_with_deps(("chunk", g), acc, move |_cx2| {
                        let items: Vec<Element> = (lo..hi)
                            .map(|i| {
                                let mut e: Element = widgets::text(format!(
                                    "row {i} · {}",
                                    vals[i - lo]
                                ));
                                for _ in 0..depth {
                                    e = widgets::column(vec![e]);
                                }
                                e
                            })
                            .collect();
                        widgets::column(items)
                    })
                })
                .collect();
            let mut root: Element = widgets::column(groups);
            if std::env::var("NOFILL").is_err() {
                root.style.width = lumen_layout::Dim::pct(1.0);
            }
            return root;
        }
        // MODE=chunkbind: chunk-scoped structure with BOUND row text. The
        // scope's deps are constant, so a row change never re-runs the chunk —
        // it patches. With GROW=1 every write declines (the string widens),
        // which is the decline cliff: pre-MUT1, one declining binding dropped
        // ALL view caches, so the next rebuild spliced nothing.
        if m == "chunkbind" {
            let chunk = std::env::var("CHUNK")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(64)
                .max(1);
            let grow = std::env::var("GROW").is_ok();
            let groups: Vec<Element> = (0..rows.div_ceil(chunk))
                .map(|g| {
                    let lo = g * chunk;
                    let hi = ((g + 1) * chunk).min(rows);
                    cx.scope_with_deps(("chunkbind", g), (), move |_cx2| {
                        let items: Vec<Element> = (lo..hi)
                            .map(|i| {
                                let mut e: Element = widgets::text(bind!(rt => {
                                    let v: Signal<i64> = rt.signal(key(i), || 0);
                                    let v = v.get(rt);
                                    if grow {
                                        format!("row {i} · {}", "x".repeat(v as usize))
                                    } else {
                                        format!("row {i} · {v}")
                                    }
                                }));
                                for _ in 0..depth {
                                    e = widgets::column(vec![e]);
                                }
                                e
                            })
                            .collect();
                        widgets::column(items)
                    })
                })
                .collect();
            let mut root: Element = widgets::column(groups);
            if std::env::var("NOFILL").is_err() {
                root.style.width = lumen_layout::Dim::pct(1.0);
            }
            return root;
        }
        let kids: Vec<Element> = (0..rows)
            .map(|i| {
                // The depth wrappers belong INSIDE the scope. An earlier
                // version wrapped the scope instead, so the 8 wrappers per row
                // were rebuilt every frame (8 002 of 9 001 nodes at D=8) and
                // memoization looked useless at depth — a property of the
                // benchmark, not of the framework.
                let wrap = |mut e: Element| {
                    for _ in 0..depth {
                        e = widgets::column(vec![e]);
                    }
                    e
                };
                match m.as_str() {
                    // Control: a top-level structural read of every row.
                    "plain" => {
                        let v: Signal<i64> = cx.signal(key(i), || 0);
                        wrap(widgets::text(format!("row {i} · {}", v.get(cx.runtime()))))
                    }
                    // F1: the whole row — leaf and wrappers — is one memoized
                    // scope keyed on its own value.
                    "scope" => {
                        let v: Signal<i64> = cx.signal(key(i), || 0);
                        let cur = v.get(cx.runtime());
                        cx.scope_with_deps(i, cur, move |_cx| {
                            wrap(widgets::text(format!("row {i} · {cur}")))
                        })
                    }
                    // F3: the row's text is a binding; its read is isolated, so
                    // it should patch rather than rebuild.
                    "bind" => wrap(widgets::text(bind!(rt => {
                        let v: Signal<i64> = rt.signal(key(i), || 0);
                        let v = v.get(rt);
                        if std::env::var("GROW").is_ok() {
                            format!("row {i} · {}", "x".repeat(v as usize))
                        } else {
                            format!("row {i} · {v}")
                        }
                    }))),
                    other => panic!("unknown MODE={other}"),
                }
            })
            .collect();
        let mut root: Element = widgets::column(kids);
        // Same as fwbench: a definite containing block is T2's precondition and
        // is what virtually every real root does.
        if std::env::var("NOFILL").is_err() {
            root.style.width = lumen_layout::Dim::pct(1.0);
        }
        root
        })
    };
    let mut h = app.run_headless(Size::new(400.0, win_h));
    h.pump();
    let build_ms = t0.elapsed().as_secs_f64() * 1e3;

    // Rotate which rows change, so this is not "the same K rows forever" — that
    // would let a cache key on position rather than on value.
    //
    // But rotate only within the top of the list. Rotating over all N walked the
    // changed row off screen after ~28 frames (a 600 px viewport holds ~28 of
    // 10 000 rows), and the benchmark then reported `damage=none`: it was
    // timing frames in which nothing visible changed, and so measuring paint's
    // early-out rather than the update. `SPAN` overrides if the offscreen case
    // is what you want to measure.
    let span = env("SPAN", changed.max(25)).min(rows);
    let mut cursor = 0usize;
    let bump = |h: &mut lumen_widgets::Headless, cursor: &mut usize| {
        for _ in 0..changed {
            let i = *cursor % span;
            *cursor += 1;
            let s: Signal<i64> = h.runtime().signal(key(i), || 0);
            s.update(h.runtime(), |v| *v += 1);
        }
        h.pump()
    };

    for _ in 0..15 {
        bump(&mut h, &mut cursor);
    }
    let mut best = u128::MAX;
    let mut times = Vec::new();
    let mut rebuilt = 0u32;
    let mut copied = 0u32;
    // Damage is the paint-side counterpart of `nodes_rebuilt`: a frame that
    // re-lowers 2 nodes but repaints Full is only half incremental.
    let mut dmg = String::new();
    for _ in 0..40 {
        let t = Instant::now();
        let st = bump(&mut h, &mut cursor);
        let us = t.elapsed().as_micros();
        best = best.min(us);
        times.push(us);
        rebuilt = st.nodes_rebuilt;
        copied = st.nodes_copied;
        dmg = match st.damage {
            lumen_render::Damage::None => "none".into(),
            lumen_render::Damage::Region(r) => {
                format!("region({:.0}x{:.0})", r.width(), r.height())
            }
            lumen_render::Damage::Full => "full".into(),
        };
    }
    times.sort_unstable();
    let med = times[times.len() / 2];
    let nodes = h.pump().node_count;

    // EQUIVALENCE GUARD. Three modes that produce different frame times are
    // only comparable if they produce the same *frame*. A mode whose binding
    // silently never fires would look fastest, which is exactly the failure
    // this benchmark exists to detect — so assert the update actually landed
    // before reporting a number for it.
    let probe = 0usize;
    let v: Signal<i64> = h.runtime().signal(key(probe), || 0);
    let want = v.get(h.runtime()) + 1;
    v.set(h.runtime(), want);
    h.pump();
    h.pump(); // restyle-drops-bindings: a moved signal can lag one frame
    let grow_text = std::env::var("GROW").is_ok() && (mode == "bind" || mode == "chunkbind");
    let want_text = if grow_text {
        format!("row {probe} · {}", "x".repeat(want as usize))
    } else {
        format!("row {probe} · {want}")
    };
    let doc = h.semantics_json().to_string();
    assert!(
        doc.contains(&want_text),
        "MODE={mode} did not apply the update — {want_text:?} is absent, so this \
         mode's frame time measures a frame that never changed"
    );
    h.assert_view_coherent();

    println!(
        "sparse\tmode={mode}\tN={rows}\tK={changed}\tD={depth}\tbuild_ms={build_ms:.1}\t\
         frame_min_us={best}\tframe_med_us={med}\tnodes_rebuilt={rebuilt}\tnodes_copied={copied}\tdamage={dmg}\t\
         span={span}\tnodes={nodes}\trss_kb={}",
        rss_kb()
    );
}
