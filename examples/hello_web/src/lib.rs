//! `hello_web` — the hello app as a WASM module exposing a tiny C ABI the
//! browser (or node) calls to render frames into WASM linear memory (T5.1).

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

/// C ABI: render the app at `w`×`h` and return a pointer to a leaked `w*h*4`
/// straight-RGBA8 buffer in WASM linear memory. The caller reads
/// `w*h*4` bytes at the returned offset from `exports.memory`.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn lumen_web_render(w: u32, h: u32) -> *const u8 {
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    lumen_shell_web::render_into(app, w, h, None, &mut buf);
    let ptr = buf.as_ptr();
    std::mem::forget(buf); // leak: the JS host owns the read
    ptr
}
