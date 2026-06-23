//! Phase **R0.3** — the damage contract: `render_damage(dl, dirty)` must be
//! byte-identical to a full `render(dl)` cropped to `dirty`, for every corpus
//! scene and a range of integer-aligned dirty rects.
//!
//! This is the invariant Phase R2 (incremental layout + retained display list +
//! present-changed-tiles) must preserve: rendering only the damaged region can
//! never observably change output. A meta-test proves the equivalence check
//! actually catches a mismatch.

mod common;

use common::*;
use kurbo::Rect;
use lumen_render::cpu;

/// Integer-aligned dirty rects to probe (within the W×H frame).
fn dirty_rects() -> Vec<(u32, u32, u32, u32)> {
    vec![
        (0, 0, W, H),             // whole frame
        (20, 16, 40, 32),         // interior block
        (0, 0, 50, 50),           // top-left corner
        (W - 40, H - 30, 40, 30), // bottom-right corner
        (90, 60, 30, 30),         // small center tile
    ]
}

#[test]
fn damage_equals_full_render_cropped_across_corpus() {
    for s in corpus() {
        let full = cpu::render(&s.dl, W, H, bg());
        for (x, y, w, h) in dirty_rects() {
            let dirty = Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64);
            let damaged = cpu::render_damage(&s.dl, W, H, bg(), dirty);
            assert_eq!(
                damaged.width(),
                w,
                "{} damage width @ {:?}",
                s.name,
                (x, y, w, h)
            );
            assert_eq!(damaged.height(), h, "{} damage height", s.name);
            let cropped = full.crop(x, y, w, h);
            assert_frames_exact(
                &damaged,
                &cropped,
                &format!("{} damage @ ({x},{y},{w},{h})", s.name),
            );
        }
    }
}

#[test]
#[should_panic(expected = "pixels differ")]
fn equivalence_check_catches_a_wrong_crop() {
    // A scene with strong spatial variation, compared against the WRONG offset:
    // the check must reject it, proving the test is meaningful.
    let dl = corpus()
        .into_iter()
        .find(|s| s.name == "path")
        .expect("path scene")
        .dl;
    let full = cpu::render(&dl, W, H, bg());
    let dirty = Rect::new(20.0, 16.0, 60.0, 48.0); // 40x32 at (20,16)
    let damaged = cpu::render_damage(&dl, W, H, bg(), dirty);
    let wrong = full.crop(24, 20, 40, 32); // shifted by (4,4)
    assert_frames_exact(&damaged, &wrong, "deliberately-wrong-offset");
}
