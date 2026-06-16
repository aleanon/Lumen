//! iOS shell (T3.3) — headless-verifiable core + a C-ABI the UIKit/Metal host
//! calls.
//!
//! iOS binaries can only be linked on macOS with Xcode, so this crate is split:
//!
//! * [`render_into`] — the **platform-independent**
//!   render core. It runs the CPU reference renderer (the same one the desktop
//!   and Android shells use) and writes straight-RGBA8 into a caller buffer.
//!   This is fully testable on the host, which is the "headless verification"
//!   the milestone asks for on a non-mac box.
//! * `ios/` — an Xcode project template (Info.plist + an Objective-C `UIView`
//!   backed by a `CAMetalLayer`) that drives a `MTLTexture` from the bytes this
//!   core produces, forwards `UITouch`es, honours safe-area insets, and bridges
//!   `UITextInput` for IME. It is built by `scripts/ios_orchestrate.sh` on a
//!   macOS runner; see [`README`](https://example.invalid) in `ios/`.
//!
//! The example `examples/hello_ios` wires a concrete app to a stable C ABI
//! (`lumen_ios_render`) that the template calls each frame.

#![warn(missing_docs)]

use lumen::{App, BuildCx, Element};
use lumen_core::geometry::Size;
use lumen_render::RgbaImage;

/// Render `build` at `w`×`h` (optionally with a `.lss` stylesheet) into `out`,
/// which must be `w*h*4` bytes. Returns the number of bytes written, or 0 if the
/// buffer is too small. The presenter blits these straight-RGBA8 bytes into a
/// Metal texture / `CGImage`.
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
