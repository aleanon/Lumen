//! MOD5: the parts of a platform shell that are not platform-specific.
//!
//! `lumen-shell-ios` and `lumen-shell-web` had independently grown the same
//! one-shot render entry point, character-for-character. Android's blit path is
//! genuinely platform-specific and stays where it is; this crate holds only
//! what is actually shared.
//!
//! The risk being removed is not line count. Duplicated platform code rots
//! asymmetrically: a fix lands on whichever platform reported the bug, and the
//! other copy keeps the defect until someone hits it there too. The 2026-08
//! modularity review found exactly that shape — iOS lacking key, wheel and
//! agent-bridge support that web already had.
#![warn(missing_docs)]

use kurbo::Size;
use lumen::{App, BuildCx, Element};
use lumen_render::RgbaImage;

/// Render one frame of `build` into `out` as RGBA8, returning bytes written.
///
/// Returns `0` if `out` is too small — a short buffer is a host-side sizing
/// mistake, and writing a partial frame would hand back something that looks
/// like an image and isn't.
///
/// This is the one-shot path: build, pump once, screenshot. Hosts that need a
/// persistent session (input, animation, an agent bridge) keep their own
/// `Headless`, because the lifecycle around it — how frames are driven, how
/// input arrives — is exactly the platform-specific part.
pub fn render_into(
    build: impl Fn(&mut BuildCx) -> Element + 'static,
    w: u32,
    h: u32,
    lss: Option<&str>,
    out: &mut [u8],
) -> usize {
    let need = (w as usize) * (h as usize) * 4;
    if out.len() < need {
        return 0;
    }
    let mut app = App::new(build);
    if let Some(src) = lss {
        app = app.stylesheet(src);
    }
    let mut hl = app.run_headless(Size::new(w as f64, h as f64));
    hl.pump();
    let frame: RgbaImage = hl.screenshot();
    let px = frame.pixels();
    let n = need.min(px.len());
    out[..n].copy_from_slice(&px[..n]);
    n
}
