//! T3.3 headless verification: the iOS render core produces correct frames at
//! iPhone resolutions and honours a `.lss` (the same tier-1 path the simulator
//! orchestration drives). Device/simulator runs happen on a macOS runner via
//! `scripts/ios_orchestrate.sh`; this is the part verifiable on any host.

use lumen::{widgets, BuildCx, Element};
use lumen_shell_ios::render_into;

fn hello(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}

fn red_pixels(buf: &[u8]) -> usize {
    buf.chunks_exact(4)
        .filter(|p| p[0] > 150 && p[1] < 90 && p[2] < 90)
        .count()
}

#[test]
fn renders_at_iphone_resolution() {
    // iPhone 15 logical-ish portrait buffer.
    let (w, h) = (1179u32, 2556u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let n = render_into(hello, w, h, None, &mut buf);
    assert_eq!(n, buf.len(), "full frame written");

    // Not blank: the hello content draws some non-white pixels.
    let non_white = buf
        .chunks_exact(4)
        .filter(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
        .count();
    assert!(
        non_white > 100,
        "expected rendered content, got {non_white} px"
    );
}

#[test]
fn buffer_too_small_is_rejected() {
    let mut tiny = vec![0u8; 16];
    assert_eq!(render_into(hello, 100, 100, None, &mut tiny), 0);
}

#[test]
fn stylesheet_repaints_for_tier1() {
    let (w, h) = (400u32, 200u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    render_into(
        hello,
        w,
        h,
        Some("#screen { background: #cc1111; }"),
        &mut buf,
    );
    assert!(red_pixels(&buf) > 1000, "tier-1 .lss must repaint the root");
}

// --- P.5 --------------------------------------------------------------

#[test]
fn session_touch_mutates_state_between_frames() {
    use lumen::widgets;
    let build = |cx: &mut lumen::BuildCx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("n={v}")).id("n"),
            widgets::button("tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
        ])
    };
    let (w, h) = (200u32, 120u32);
    let mut a = vec![0u8; (w * h * 4) as usize];
    assert!(lumen_shell_ios::session_render(build, w, h, None, &mut a) > 0);

    // Tap roughly where the button sits (below the label).
    lumen_shell_ios::session_touch(0, 30.0, 45.0);
    lumen_shell_ios::session_touch(2, 30.0, 45.0);

    let mut b = vec![0u8; (w * h * 4) as usize];
    assert!(lumen_shell_ios::session_render(build, w, h, None, &mut b) > 0);
    assert_ne!(a, b, "the tap changed the rendered frame (state persisted)");
}
