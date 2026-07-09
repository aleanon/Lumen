//! T.3 (docs/plan-remediation-2026-07.md): a golden mismatch writes
//! `.actual.png` **and** `.diff.png` (differing pixels red over a dimmed
//! base), and the perceptual compare (`expect_screenshot_within`, 05 §4)
//! passes AA-scale noise while failing real divergence.
//!
//! Uses a private LUMEN_GOLDEN_DIR temp dir. The env var is process-global,
//! so this file keeps everything in ONE #[test] (cargo runs test *files* in
//! separate processes; functions within a file share ours).

use lumen_render::diff::Tolerance;
use lumen_test::{block_on, Size, TestApp};
use lumen_widgets::{widgets, App};

fn app(label: &'static str) -> App {
    App::new(move |_cx| widgets::column(vec![widgets::text(label).id("t")]))
}

#[test]
fn mismatch_writes_actual_and_diff_and_perceptual_tolerates() {
    let dir = std::env::temp_dir().join(format!("lumen-golden-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("LUMEN_GOLDEN_DIR", &dir);

    block_on(async {
        // Record a golden from app A…
        std::env::set_var("LUMEN_UPDATE_GOLDENS", "1");
        let mut a = TestApp::with_size(app("hello"), Size::new(200.0, 80.0));
        a.pump_until_idle().await;
        a.expect_screenshot("t3").await;
        std::env::remove_var("LUMEN_UPDATE_GOLDENS");

        // …then compare app B (different text) against it: must fail and
        // write both artifacts.
        let mut b = TestApp::with_size(app("goodbye"), Size::new(200.0, 80.0));
        b.pump_until_idle().await;
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            block_on(async { b.expect_screenshot("t3").await })
        }));
        assert!(panicked.is_err(), "mismatch must fail");
        assert!(dir.join("t3.actual.png").exists(), "actual written");
        assert!(dir.join("t3.diff.png").exists(), "diff written");

        // Perceptual: identical frames pass trivially…
        let mut a2 = TestApp::with_size(app("hello"), Size::new(200.0, 80.0));
        a2.pump_until_idle().await;
        a2.expect_screenshot_within("t3", Tolerance::AA).await;
        // …and the divergent app fails the tight PARITY budget (the small
        // text swap differs on ~2–3% of this frame — inside AA's 4% seam
        // allowance by design, but far over PARITY's 0.5%).
        let mut b2 = TestApp::with_size(app("goodbye"), Size::new(200.0, 80.0));
        b2.pump_until_idle().await;
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            block_on(async { b2.expect_screenshot_within("t3", Tolerance::PARITY).await })
        }));
        assert!(panicked.is_err(), "real divergence fails perceptually");
    });

    std::fs::remove_dir_all(&dir).ok();
    std::env::remove_var("LUMEN_GOLDEN_DIR");
}
