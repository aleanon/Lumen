//! `hello_ios` — the M3 hello app exposed over a stable C ABI for the iOS
//! Metal host (T3.3). Host-compilable, so the ABI is verified to build/run on
//! any platform; the actual UIKit/Metal app is built on macOS from the template
//! in `crates/lumen-shell-ios/ios/`.

use lumen::{widgets, BuildCx, Element};

/// The hello app (kept in sync with the other shells).
pub fn app(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}

/// C ABI: render the app at `w`×`h` into `out` (`w*h*4` straight-RGBA8 bytes).
/// Returns the number of bytes written (0 on bad args). The Metal host calls
/// this each frame and uploads the bytes into an `MTLTexture`.
///
/// # Safety
/// `out` must be valid for writes of `out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn lumen_ios_render(w: u32, h: u32, out: *mut u8, out_len: usize) -> usize {
    if out.is_null() {
        return 0;
    }
    let buf = std::slice::from_raw_parts_mut(out, out_len);
    // P.5: the persistent session — state survives across frames, so
    // touches (below) actually mutate the app the next frame renders.
    lumen_shell_ios::session_render(app, w, h, None, buf)
}

/// C ABI: a touch event — `phase` 0 = began, 1 = moved, 2 = ended; `x`/`y`
/// in logical points. The template's UITouch handlers call this (P.5 — this
/// symbol was referenced by the template before it existed).
#[no_mangle]
pub extern "C" fn lumen_ios_touch(phase: u32, x: f64, y: f64) {
    lumen_shell_ios::session_touch(phase, x, y);
}

/// C ABI: committed text from the UITextInput bridge (IME).
///
/// # Safety
/// `utf8` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn lumen_ios_text(utf8: *const std::ffi::c_char) {
    if utf8.is_null() {
        return;
    }
    if let Ok(s) = std::ffi::CStr::from_ptr(utf8).to_str() {
        lumen_shell_ios::session_text(s);
    }
}
