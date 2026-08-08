//! MOD5: the shared one-shot render path.
//!
//! Both platform shells delegate here, so this is the one place the behaviour
//! is pinned — previously each had its own copy and neither had a test, which
//! is how iOS ended up missing key, wheel and agent-bridge support that web
//! already had.

use lumen_shell_core::render_into;
use lumen_widgets::{widgets, BuildCx, Element};

fn some_text(_cx: &mut BuildCx) -> Element {
    widgets::text("hi")
}

#[test]
fn renders_a_frame_into_the_buffer() {
    let (w, h) = (32u32, 16u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let n = render_into(some_text, w, h, None, &mut buf);
    assert_eq!(n, buf.len(), "a full frame should be written");
    assert!(
        buf.iter().any(|&b| b != 0),
        "the buffer should contain actual pixels"
    );
}

#[test]
fn a_short_buffer_writes_nothing() {
    // Returning 0 rather than a partial frame is deliberate: a short buffer is
    // a host-side sizing mistake, and half an image looks like a rendering bug
    // at the call site instead of the allocation error it is.
    let (w, h) = (32u32, 16u32);
    let mut buf = vec![7u8; ((w * h * 4) - 4) as usize];
    let n = render_into(some_text, w, h, None, &mut buf);
    assert_eq!(n, 0, "an undersized buffer must be refused");
    assert!(
        buf.iter().all(|&b| b == 7),
        "and must be left untouched, not partially filled"
    );
}

#[test]
fn a_stylesheet_is_applied() {
    let (w, h) = (16u32, 8u32);
    let mut plain = vec![0u8; (w * h * 4) as usize];
    render_into(some_text, w, h, None, &mut plain);
    let mut styled = vec![0u8; (w * h * 4) as usize];
    render_into(
        some_text,
        w,
        h,
        Some("text { color: #ff0000ff; }"),
        &mut styled,
    );
    assert_ne!(
        plain, styled,
        "the `lss` argument must reach the render, not be silently dropped"
    );
}
