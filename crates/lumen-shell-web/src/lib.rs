//! Web / WASM shell (T5.1) — the platform-independent render core plus a
//! canvas presenter template.
//!
//! The framework's CPU reference renderer compiles to `wasm32-unknown-unknown`
//! (after dropping fontique's `system` backend), so a Lumen app renders the
//! exact same pixels in the browser as on the desktop CPU renderer — the basis
//! for cross-platform golden parity. [`render_into`] is that shared core; an app
//! exposes it over a tiny C ABI (see `examples/hello_web`) and the JS loader in
//! `web/` blits the bytes into a `<canvas>` and bridges `lumen-agent`. The
//! GPU/WebGPU presenter is the production path; CPU-to-canvas is the fallback
//! and the deterministic golden path.

#![warn(missing_docs)]

use lumen::{App, BuildCx, Element};
use lumen_core::geometry::Size;
use lumen_render::RgbaImage;

/// Render `build` at `w`×`h` (optionally with a `.lss` stylesheet) into `out`
/// (`w*h*4` straight-RGBA8 bytes). Returns bytes written, or 0 if `out` is too
/// small. The canvas presenter uploads these bytes into `ImageData`.
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
