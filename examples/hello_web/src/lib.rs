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

// --- P.2: persistent session ABI ---------------------------------------------
// The browser/node host drives one live session: input → the one queue, a RAF
// tick renders only changed frames, and the agent bridge dispatches JSON-RPC.
// Buffers cross the boundary as (ptr, len) into linear memory; JS allocates
// request bytes via `lumen_web_alloc`.
#[cfg(target_arch = "wasm32")]
mod web_abi {
    use std::cell::RefCell;

    thread_local! {
        static FRAME: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        static REPLY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    /// Boot (or resize) the persistent session. `w`/`h` in CSS px.
    #[no_mangle]
    pub extern "C" fn lumen_web_start(w: f64, h: f64, scale: f64) {
        lumen_shell_web::session_start(super::app, w, h, scale, None);
        let (pw, ph) = lumen_shell_web::session_frame_size();
        FRAME.with(|f| f.borrow_mut().resize((pw as usize) * (ph as usize) * 4, 0));
    }

    /// Pointer input in CSS px. `phase`: 0 down, 1 move, 2 up.
    #[no_mangle]
    pub extern "C" fn lumen_web_pointer(phase: u32, x: f64, y: f64) {
        lumen_shell_web::session_pointer(phase, x, y);
    }

    /// Named key (see `session_key` codes); flags are 0/1.
    #[no_mangle]
    pub extern "C" fn lumen_web_key(code: u32, down: u32, shift: u32, ctrl: u32) {
        lumen_shell_web::session_key(code, down != 0, shift != 0, ctrl != 0);
    }

    /// Wheel at a CSS-px position.
    #[no_mangle]
    pub extern "C" fn lumen_web_wheel(x: f64, y: f64, dx: f64, dy: f64) {
        lumen_shell_web::session_wheel(x, y, dx, dy);
    }

    /// Allocate `n` bytes in linear memory for a JS→wasm payload; the next
    /// `lumen_web_text`/`lumen_web_agent` call consumes and frees it.
    #[no_mangle]
    pub extern "C" fn lumen_web_alloc(n: usize) -> *mut u8 {
        let mut v = Vec::with_capacity(n);
        let p = v.as_mut_ptr();
        std::mem::forget(v);
        p
    }

    unsafe fn take_str(ptr: *mut u8, len: usize) -> String {
        let v = Vec::from_raw_parts(ptr, len, len);
        String::from_utf8_lossy(&v).into_owned()
    }

    /// Committed text input (UTF-8 at `ptr`, from `lumen_web_alloc`).
    ///
    /// # Safety
    /// `ptr`/`len` must be exactly the allocation returned by
    /// [`lumen_web_alloc`]`(len)`, fully initialized.
    #[no_mangle]
    pub unsafe extern "C" fn lumen_web_text(ptr: *mut u8, len: usize) {
        lumen_shell_web::session_text(&take_str(ptr, len));
    }

    /// One RAF tick: advance by `dt_ms`, pump, render if changed. Returns the
    /// frame byte count written to the buffer at [`lumen_web_frame_ptr`]
    /// (0 ⇒ idle frame, canvas keeps its contents).
    #[no_mangle]
    pub extern "C" fn lumen_web_frame(dt_ms: f64) -> usize {
        FRAME.with(|f| {
            let mut buf = f.borrow_mut();
            let (pw, ph) = lumen_shell_web::session_frame_size();
            let need = (pw as usize) * (ph as usize) * 4;
            if buf.len() != need {
                buf.resize(need, 0);
            }
            lumen_shell_web::session_frame(dt_ms, &mut buf)
        })
    }

    /// Base address of the frame buffer (valid after `lumen_web_frame`).
    #[no_mangle]
    pub extern "C" fn lumen_web_frame_ptr() -> *const u8 {
        FRAME.with(|f| f.borrow().as_ptr())
    }

    /// Physical frame width/height (canvas dimensions).
    #[no_mangle]
    pub extern "C" fn lumen_web_width() -> u32 {
        lumen_shell_web::session_frame_size().0
    }
    /// Physical frame height.
    #[no_mangle]
    pub extern "C" fn lumen_web_height() -> u32 {
        lumen_shell_web::session_frame_size().1
    }

    /// Whether the UI wants another frame (animations) — RAF idles otherwise.
    #[no_mangle]
    pub extern "C" fn lumen_web_needs_frame() -> u32 {
        u32::from(lumen_shell_web::session_needs_frame())
    }

    /// Agent bridge: dispatch one JSON-RPC line (UTF-8 request at `ptr`).
    /// Returns the reply's byte length; read it at [`lumen_web_reply_ptr`].
    ///
    /// # Safety
    /// `ptr`/`len` as for [`lumen_web_text`].
    #[no_mangle]
    pub unsafe extern "C" fn lumen_web_agent(ptr: *mut u8, len: usize) -> usize {
        let req = take_str(ptr, len);
        let resp = lumen_shell_web::session_agent(&req);
        REPLY.with(|r| {
            let mut buf = r.borrow_mut();
            buf.clear();
            buf.extend_from_slice(resp.as_bytes());
            buf.len()
        })
    }

    /// Base address of the last agent reply.
    #[no_mangle]
    pub extern "C" fn lumen_web_reply_ptr() -> *const u8 {
        REPLY.with(|r| r.borrow().as_ptr())
    }
}
