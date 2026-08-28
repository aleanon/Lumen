//! L1: is the exponential nesting cost taffy's, or Lumen's use of taffy?
//!
//! Pure taffy, no Lumen types. Same shape as `buildphase DEPTH=n`: a column of
//! `ROWS` rows, each row wrapped in `depth` auto-sized flex columns, innermost
//! a fixed-size leaf. If this explodes, the cost is taffy's (or its cache's)
//! and Lumen's wrapper is not implicated; if it stays linear, Lumen is doing
//! something that defeats the cache.

use std::time::Instant;
use taffy::prelude::*;
use taffy::LayoutOutput;

const ROWS: usize = 100;

/// Which container style is being tested. The point is to find a property
/// Lumen could set by default on its own containers, so the fix does not
/// require patching or upgrading taffy.
fn wrapper(v: &str) -> Style {
    let mut s = Style {
        display: Display::Flex,
        flex_direction: if std::env::var("ROWDIR").is_ok() {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        ..Default::default()
    };
    match v {
        "auto" => {}
        "w100" => s.size.width = percent(1.0),
        "h100" => s.size.height = percent(1.0),
        "wh100" => {
            s.size.width = percent(1.0);
            s.size.height = percent(1.0);
        }
        "grow" => s.flex_grow = 1.0,
        "wdef" => s.size.width = length(400.0),
        "hdef" => s.size.height = length(18.0),
        "both_def" => {
            s.size.width = length(400.0);
            s.size.height = length(18.0);
        }
        "basis" => s.flex_basis = auto(),
        "minmax0" => s.min_size.height = length(0.0),
        // `align-items: stretch` is already flexbox's default, so if taffy
        // honoured it as a definite cross size these would collapse too — and
        // the Lumen-side fix would be one line rather than a width policy.
        "self_stretch" => s.align_self = Some(AlignSelf::STRETCH),
        "items_stretch" => s.align_items = Some(AlignItems::STRETCH),
        "both_stretch" => {
            s.align_self = Some(AlignSelf::STRETCH);
            s.align_items = Some(AlignItems::STRETCH);
        }
        "minw100" => s.min_size.width = percent(1.0),
        other => panic!("unknown variant {other}"),
    }
    s
}

/// Leaves carry a context so a measure function can COUNT how many times each
/// one is asked for its size. Exponential wall-clock with a per-node cache is
/// only explicable if the cache is not preventing re-measurement, and this is
/// the direct way to see that rather than infer it.
fn build(depth: usize, v: &str) -> (TaffyTree<()>, NodeId) {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let rows: Vec<NodeId> = (0..ROWS)
        .map(|_| {
            let mut n = t
                .new_leaf_with_context(
                    Style {
                        size: Size {
                            width: length(120.0),
                            height: length(18.0),
                        },
                        ..Default::default()
                    },
                    (),
                )
                .unwrap();
            for _ in 0..depth {
                n = t.new_with_children(wrapper(v), &[n]).unwrap();
            }
            n
        })
        .collect();
    let root = t.new_with_children(wrapper(v), &rows).unwrap();
    (t, root)
}

/// One timed layout of the given depth/variant; returns (min_us, measures).
fn run(depth: usize, v: &str) -> (f64, usize) {
    use std::sync::atomic::Ordering;
    let mut best = f64::MAX;
    let mut measures = 0;
    for _ in 0..5 {
        let (mut t, root) = build(depth, v);
        MEASURES.store(0, Ordering::Relaxed);
        let s = Instant::now();
        t.compute_layout_with_measure(
            root,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::Definite(600.0),
            },
            |known, _a, _i, _c| {
                MEASURES.fetch_add(1, Ordering::Relaxed);
                LayoutOutput::from_outer_size(Size {
                    width: known.known_dimensions.width.unwrap_or(120.0),
                    height: known.known_dimensions.height.unwrap_or(18.0),
                })
            },
        )
        .unwrap();
        let us = s.elapsed().as_secs_f64() * 1e6;
        if us < best {
            best = us;
            measures = MEASURES.load(Ordering::Relaxed);
        }
    }
    (best, measures)
}

static MEASURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn main() {
    let variants = [
        "auto",
        "w100",
        "self_stretch",
        "items_stretch",
        "both_stretch",
        "minw100",
    ];
    if let Some(v) = std::env::args().nth(1) {
        if v == "matrix" {
            println!("variant\tdepth8_us\tmeasures/leaf");
            for v in variants {
                let (us, m) = run(8, v);
                println!("{v}\t{us:.1}\t{:.1}", m as f64 / ROWS as f64);
            }
            return;
        }
    }
    println!("depth\tnodes\tmin_us\tus/node\tmeasures\tper_leaf");
    for depth in [0usize, 2, 4, 6, 8, 10] {
        let mut best = f64::MAX;
        let mut nodes = 0;
        let mut measures = 0;
        for _ in 0..5 {
            let (mut t, root) = build(depth, "auto");
            nodes = t.total_node_count();
            let s = Instant::now();
            MEASURES.store(0, std::sync::atomic::Ordering::Relaxed);
            t.compute_layout_with_measure(
                root,
                Size {
                    width: AvailableSpace::Definite(400.0),
                    height: AvailableSpace::Definite(600.0),
                },
                |known, _avail, _id, _ctx| {
                    MEASURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    LayoutOutput::from_outer_size(Size {
                        width: known.known_dimensions.width.unwrap_or(120.0),
                        height: known.known_dimensions.height.unwrap_or(18.0),
                    })
                },
            )
            .unwrap();
            measures = MEASURES.load(std::sync::atomic::Ordering::Relaxed);
            let us = s.elapsed().as_secs_f64() * 1e6;
            if us < best {
                best = us;
            }
        }
        println!(
            "{depth}\t{nodes}\t{best:.1}\t{:.2}\t{measures}\t{:.1}",
            best / nodes as f64,
            measures as f64 / ROWS as f64
        );
    }
}
