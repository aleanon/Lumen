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

// --- P.5: the persistent session the touch/IME FFI needs ------------------

use lumen_core::events::{Event, PointerEvent, TextInputEvent};
use std::cell::RefCell;

thread_local! {
    /// The C-ABI session: one live app per host thread. `lumen_ios_render`
    /// was previously stateless (a fresh app every frame), which made the
    /// touch/IME entry points the template referenced impossible — state
    /// now survives across frames, so taps actually increment counters.
    static SESSION: RefCell<Option<(lumen::Headless, u32, u32)>> = const { RefCell::new(None) };
}

/// Render one frame of the persistent session (P.5): boots the app on first
/// call (or after a size change — a rotate re-creates; state survives via the
/// snapshot handoff on `snapshot` builds), pumps, and copies straight-RGBA8
/// into `out`. Returns bytes written, 0 if the buffer is too small.
pub fn session_render(
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
    SESSION.with(|s| {
        let mut slot = s.borrow_mut();
        let rebuild = !matches!(&*slot, Some((_, sw, sh)) if *sw == w && *sh == h);
        if rebuild {
            #[cfg(feature = "snapshot")]
            let snap = slot.as_ref().map(|(hl, _, _)| hl.snapshot());
            let mut app = App::new(build);
            if let Some(src) = lss {
                app = app.stylesheet(src);
            }
            let size = Size::new(w as f64, h as f64);
            #[cfg(feature = "snapshot")]
            let hl = match snap {
                Some(snap) => app.run_headless_restored(size, snap).0,
                None => app.run_headless(size),
            };
            #[cfg(not(feature = "snapshot"))]
            let hl = app.run_headless(size);
            *slot = Some((hl, w, h));
        }
        let (hl, _, _) = slot.as_mut().expect("session initialized above");
        hl.pump();
        let frame: RgbaImage = hl.screenshot();
        let px = frame.pixels();
        let n = need.min(px.len());
        out[..n].copy_from_slice(&px[..n]);
        n
    })
}

/// Touch phase for [`session_touch`]: 0 = down, 1 = move, 2 = up.
pub fn session_touch(phase: u32, x: f64, y: f64) {
    SESSION.with(|s| {
        if let Some((hl, _, _)) = s.borrow_mut().as_mut() {
            let p = lumen_core::geometry::Point::new(x, y);
            let ev = match phase {
                0 => Event::PointerDown(PointerEvent::at(p)),
                1 => Event::PointerMove(PointerEvent::at(p)),
                _ => Event::PointerUp(PointerEvent::at(p)),
            };
            hl.inject(ev);
            hl.pump();
        }
    });
}

/// Committed text (UITextInput bridge) into the focused editor.
pub fn session_text(text: &str) {
    SESSION.with(|s| {
        if let Some((hl, _, _)) = s.borrow_mut().as_mut() {
            hl.inject(Event::TextInput(TextInputEvent { text: text.into() }));
            hl.pump();
        }
    });
}
