//! T0.5 acceptance: a 40-fixture suite asserting exact bounds across flex,
//! grid, absolute, min/max, and aspect-ratio, plus a dirty-subtree relayout
//! that touches only descendant nodes.

use kurbo::{Rect, Size};
use lumen_layout::style::*;
use lumen_layout::{LayoutNode, LayoutTree};

fn base() -> LayoutStyle {
    LayoutStyle::default()
}
fn sized(w: f32, h: f32) -> LayoutStyle {
    LayoutStyle {
        width: Dim::px(w),
        height: Dim::px(h),
        ..base()
    }
}
fn grow() -> LayoutStyle {
    LayoutStyle {
        flex_grow: 1.0,
        ..base()
    }
}

struct Checker {
    n: usize,
}
impl Checker {
    fn eq(&mut self, got: Rect, x: f64, y: f64, w: f64, h: f64) {
        let ok = (got.x0 - x).abs() < 0.02
            && (got.y0 - y).abs() < 0.02
            && (got.width() - w).abs() < 0.02
            && (got.height() - h).abs() < 0.02;
        assert!(ok, "bounds {got:?} != ({x}, {y}, {w}, {h})");
        self.n += 1;
    }
}

/// Build a row/column container of the given size with the given children.
fn run(
    style: LayoutStyle,
    children: Vec<LayoutStyle>,
    avail: (f64, f64),
) -> (LayoutTree, LayoutNode, Vec<LayoutNode>) {
    let mut t = LayoutTree::new();
    let kids: Vec<LayoutNode> = children.into_iter().map(|s| t.leaf(s)).collect();
    let root = t.container(style, &kids);
    t.compute(root, Size::new(avail.0, avail.1));
    (t, root, kids)
}

#[test]
fn forty_layout_fixtures() {
    let mut c = Checker { n: 0 };

    // --- flex row: grow distribution -------------------------------------
    let (t, _r, k) = run(sized(300.0, 100.0), vec![grow(), grow()], (300.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 150.0, 100.0);
    c.eq(t.bounds(k[1]), 150.0, 0.0, 150.0, 100.0);

    // fixed + grow
    let (t, _r, k) = run(
        sized(300.0, 100.0),
        vec![sized(100.0, 100.0), grow()],
        (300.0, 100.0),
    );
    c.eq(t.bounds(k[0]), 0.0, 0.0, 100.0, 100.0);
    c.eq(t.bounds(k[1]), 100.0, 0.0, 200.0, 100.0);

    // --- flex column -----------------------------------------------------
    let col = LayoutStyle {
        flex_direction: FlexDirection::Column,
        ..sized(100.0, 300.0)
    };
    let (t, _r, k) = run(col, vec![grow(), grow()], (100.0, 300.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 100.0, 150.0);
    c.eq(t.bounds(k[1]), 0.0, 150.0, 100.0, 150.0);

    // --- gap -------------------------------------------------------------
    let gapped = LayoutStyle {
        column_gap: Dim::px(20.0),
        ..sized(300.0, 100.0)
    };
    let (t, _r, k) = run(gapped, vec![grow(), grow()], (300.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 140.0, 100.0);
    c.eq(t.bounds(k[1]), 160.0, 0.0, 140.0, 100.0);

    // --- justify_content -------------------------------------------------
    let jc = LayoutStyle {
        justify_content: Some(Align::Center),
        ..sized(300.0, 100.0)
    };
    let (t, _r, k) = run(jc, vec![sized(100.0, 100.0)], (300.0, 100.0));
    c.eq(t.bounds(k[0]), 100.0, 0.0, 100.0, 100.0);

    let jsb = LayoutStyle {
        justify_content: Some(Align::SpaceBetween),
        ..sized(300.0, 100.0)
    };
    let (t, _r, k) = run(
        jsb,
        vec![sized(50.0, 100.0), sized(50.0, 100.0)],
        (300.0, 100.0),
    );
    c.eq(t.bounds(k[0]), 0.0, 0.0, 50.0, 100.0);
    c.eq(t.bounds(k[1]), 250.0, 0.0, 50.0, 100.0);

    // --- align_items -----------------------------------------------------
    let ai_center = LayoutStyle {
        align_items: Some(Align::Center),
        ..sized(200.0, 100.0)
    };
    let (t, _r, k) = run(ai_center, vec![sized(40.0, 40.0)], (200.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 30.0, 40.0, 40.0);

    let ai_end = LayoutStyle {
        align_items: Some(Align::End),
        ..sized(200.0, 100.0)
    };
    let (t, _r, k) = run(ai_end, vec![sized(40.0, 40.0)], (200.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 60.0, 40.0, 40.0);

    // --- padding ---------------------------------------------------------
    let padded = LayoutStyle {
        padding: Edges::all(Dim::px(20.0)),
        ..sized(200.0, 200.0)
    };
    let (t, _r, k) = run(padded, vec![grow()], (200.0, 200.0));
    c.eq(t.bounds(k[0]), 20.0, 20.0, 160.0, 160.0);

    // --- margin ----------------------------------------------------------
    let mk = LayoutStyle {
        margin: Edges::all(Dim::px(10.0)),
        ..sized(50.0, 50.0)
    };
    let (t, _r, k) = run(sized(200.0, 200.0), vec![mk], (200.0, 200.0));
    c.eq(t.bounds(k[0]), 10.0, 10.0, 50.0, 50.0);

    // --- flex_shrink -----------------------------------------------------
    let shrink_kid = || LayoutStyle {
        width: Dim::px(80.0),
        flex_shrink: 1.0,
        ..base()
    };
    let (t, _r, k) = run(
        sized(100.0, 100.0),
        vec![shrink_kid(), shrink_kid()],
        (100.0, 100.0),
    );
    c.eq(t.bounds(k[0]), 0.0, 0.0, 50.0, 100.0);
    c.eq(t.bounds(k[1]), 50.0, 0.0, 50.0, 100.0);

    // --- min/max ---------------------------------------------------------
    let min_w = LayoutStyle {
        width: Dim::px(20.0),
        min_width: Dim::px(60.0),
        flex_shrink: 0.0,
        ..base()
    };
    let (t, _r, k) = run(sized(300.0, 100.0), vec![min_w], (300.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 60.0, 100.0);

    let max_w = LayoutStyle {
        width: Dim::px(200.0),
        max_width: Dim::px(80.0),
        ..base()
    };
    let (t, _r, k) = run(sized(300.0, 100.0), vec![max_w], (300.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 80.0, 100.0);

    let min_h = LayoutStyle {
        width: Dim::px(30.0),
        height: Dim::px(10.0),
        min_height: Dim::px(40.0),
        align_self: Some(Align::Start),
        ..base()
    };
    let (t, _r, k) = run(sized(100.0, 200.0), vec![min_h], (100.0, 200.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 30.0, 40.0);

    let max_h = LayoutStyle {
        width: Dim::px(30.0),
        height: Dim::px(300.0),
        max_height: Dim::px(80.0),
        align_self: Some(Align::Start),
        ..base()
    };
    let (t, _r, k) = run(sized(100.0, 200.0), vec![max_h], (100.0, 200.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 30.0, 80.0);

    // --- aspect-ratio ----------------------------------------------------
    let ar = LayoutStyle {
        width: Dim::px(100.0),
        aspect_ratio: Some(2.0),
        align_self: Some(Align::Start),
        ..base()
    };
    let (t, _r, k) = run(sized(300.0, 300.0), vec![ar], (300.0, 300.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 100.0, 50.0);

    // --- absolute positioning -------------------------------------------
    let abs_tl = LayoutStyle {
        position: Position::Absolute,
        inset: Edges {
            left: Dim::px(10.0),
            top: Dim::px(20.0),
            ..Edges::AUTO
        },
        width: Dim::px(50.0),
        height: Dim::px(30.0),
        ..base()
    };
    let (t, _r, k) = run(sized(200.0, 200.0), vec![abs_tl], (200.0, 200.0));
    c.eq(t.bounds(k[0]), 10.0, 20.0, 50.0, 30.0);

    let abs_br = LayoutStyle {
        position: Position::Absolute,
        inset: Edges {
            right: Dim::px(10.0),
            bottom: Dim::px(10.0),
            ..Edges::AUTO
        },
        width: Dim::px(40.0),
        height: Dim::px(40.0),
        ..base()
    };
    let (t, _r, k) = run(sized(200.0, 200.0), vec![abs_br], (200.0, 200.0));
    c.eq(t.bounds(k[0]), 150.0, 150.0, 40.0, 40.0);

    // --- nested ----------------------------------------------------------
    {
        let mut t = LayoutTree::new();
        let inner_a = t.leaf(grow());
        let inner_b = t.leaf(grow());
        let inner = t.container(
            LayoutStyle {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..base()
            },
            &[inner_a, inner_b],
        );
        let side = t.leaf(sized(100.0, 200.0));
        let root = t.container(sized(300.0, 200.0), &[side, inner]);
        t.compute(root, Size::new(300.0, 200.0));
        c.eq(t.bounds(side), 0.0, 0.0, 100.0, 200.0);
        c.eq(t.bounds(inner), 100.0, 0.0, 200.0, 200.0);
        c.eq(t.bounds(inner_a), 100.0, 0.0, 200.0, 100.0);
        c.eq(t.bounds(inner_b), 100.0, 100.0, 200.0, 100.0);
    }

    // --- wrap ------------------------------------------------------------
    let wrap = LayoutStyle {
        flex_wrap: FlexWrap::Wrap,
        width: Dim::px(100.0),
        height: Dim::px(100.0),
        align_content: Some(Align::Start),
        ..base()
    };
    let wk = || LayoutStyle {
        width: Dim::px(60.0),
        height: Dim::px(20.0),
        ..base()
    };
    let (t, _r, k) = run(wrap, vec![wk(), wk(), wk()], (100.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 60.0, 20.0);
    c.eq(t.bounds(k[1]), 0.0, 20.0, 60.0, 20.0);
    c.eq(t.bounds(k[2]), 0.0, 40.0, 60.0, 20.0);

    // --- grid ------------------------------------------------------------
    let grid = LayoutStyle {
        display: Display::Grid,
        grid_template_columns: vec![GridTrack::Px(100.0), GridTrack::Px(100.0)],
        grid_template_rows: vec![GridTrack::Px(50.0), GridTrack::Px(50.0)],
        ..sized(200.0, 100.0)
    };
    let (t, _r, k) = run(grid, vec![base(), base(), base(), base()], (200.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 100.0, 50.0);
    c.eq(t.bounds(k[1]), 100.0, 0.0, 100.0, 50.0);
    c.eq(t.bounds(k[2]), 0.0, 50.0, 100.0, 50.0);
    c.eq(t.bounds(k[3]), 100.0, 50.0, 100.0, 50.0);

    // grid with fractional (fr) tracks: 1fr + 3fr over 200px => 50 + 150
    let grid2 = LayoutStyle {
        display: Display::Grid,
        grid_template_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(3.0)],
        grid_template_rows: vec![GridTrack::Px(50.0)],
        ..sized(200.0, 50.0)
    };
    let (t, _r, k) = run(grid2, vec![base(), base()], (200.0, 50.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 50.0, 50.0);
    c.eq(t.bounds(k[1]), 50.0, 0.0, 150.0, 50.0);

    // --- percent sizing --------------------------------------------------
    let pct = LayoutStyle {
        width: Dim::pct(0.5),
        height: Dim::pct(0.25),
        align_self: Some(Align::Start),
        flex_shrink: 0.0,
        ..base()
    };
    let (t, _r, k) = run(sized(200.0, 200.0), vec![pct], (200.0, 200.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 100.0, 50.0);

    // --- row_gap in a column ---------------------------------------------
    let colgap = LayoutStyle {
        flex_direction: FlexDirection::Column,
        row_gap: Dim::px(20.0),
        ..sized(100.0, 300.0)
    };
    let (t, _r, k) = run(colgap, vec![grow(), grow()], (100.0, 300.0));
    c.eq(t.bounds(k[0]), 0.0, 0.0, 100.0, 140.0);
    c.eq(t.bounds(k[1]), 0.0, 160.0, 100.0, 140.0);

    // --- align_self overrides align_items --------------------------------
    let asc = LayoutStyle {
        align_items: Some(Align::Start),
        ..sized(200.0, 100.0)
    };
    let child_as = LayoutStyle {
        width: Dim::px(40.0),
        height: Dim::px(40.0),
        align_self: Some(Align::End),
        ..base()
    };
    let (t, _r, k) = run(asc, vec![child_as], (200.0, 100.0));
    c.eq(t.bounds(k[0]), 0.0, 60.0, 40.0, 40.0);

    assert!(c.n >= 40, "expected >= 40 fixture assertions, ran {}", c.n);
}

#[test]
fn dirty_subtree_relayout_touches_only_descendants() {
    // root [ sibling(fixed) , panel(fixed 100x100) [ a , b ] ]
    let mut t = LayoutTree::new();
    let a = t.leaf(grow());
    let b = t.leaf(grow());
    let panel = t.container(
        LayoutStyle {
            flex_direction: FlexDirection::Column,
            ..sized(100.0, 100.0)
        },
        &[a, b],
    );
    let sibling = t.leaf(sized(100.0, 100.0));
    let root = t.container(sized(200.0, 100.0), &[sibling, panel]);
    t.compute(root, Size::new(200.0, 100.0));

    let total = t.touched();
    assert_eq!(total, 5, "root + sibling + panel + a + b");
    let sibling_before = t.bounds(sibling);
    let panel_before = t.bounds(panel);
    assert_eq!(t.bounds(a), Rect::new(100.0, 0.0, 200.0, 50.0));

    // Change an internal leaf inside the fixed-size panel, then relayout only
    // the panel subtree. Its box is fixed, so nothing outside should move.
    t.set_style(
        a,
        LayoutStyle {
            flex_grow: 3.0,
            ..base()
        },
    );
    t.relayout_subtree(panel);

    assert_eq!(t.touched(), 3, "panel + a + b only");
    assert_eq!(t.bounds(sibling), sibling_before, "sibling must not move");
    assert_eq!(t.bounds(panel), panel_before, "panel box must not move");
    // a now takes 3/4 of the 100px column height.
    assert_eq!(t.bounds(a), Rect::new(100.0, 0.0, 200.0, 75.0));
    assert_eq!(t.bounds(b), Rect::new(100.0, 75.0, 200.0, 100.0));
}
