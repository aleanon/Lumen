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

// --- P.2: persistent browser session ----------------------------------------
//
// One live `Headless` per wasm instance (wasm is single-threaded, so a
// thread_local is effectively a global), driven by the JS host: input events
// feed the one queue, a requestAnimationFrame loop advances the virtual clock
// and renders only when something painted, and the agent bridge dispatches
// JSON-RPC against the same session (the browser side owns the transport —
// WebSocket in dev, CDP in the headless gate).

use lumen::Headless;
use lumen_core::events::{
    Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent, WheelEvent,
};
use lumen_core::geometry::Point;
use std::cell::RefCell;

thread_local! {
    static SESSION: RefCell<Option<Headless>> = const { RefCell::new(None) };
}

/// Boot the persistent session (idempotent — subsequent calls resize).
pub fn session_start(
    build: impl Fn(&mut BuildCx) -> Element + 'static,
    w: f64,
    h: f64,
    scale: f64,
    lss: Option<&str>,
) {
    SESSION.with(|s| {
        let mut slot = s.borrow_mut();
        match slot.as_mut() {
            Some(hl) => hl.prepare_resize(Size::new(w, h), scale),
            None => {
                let mut app = App::new(build);
                if let Some(src) = lss {
                    app = app.stylesheet(src);
                }
                let mut hl = app.run_headless(Size::new(w, h));
                hl.set_scale(scale);
                hl.pump();
                *slot = Some(hl);
            }
        }
    });
}

fn with_session<T>(f: impl FnOnce(&mut Headless) -> T) -> Option<T> {
    SESSION.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Pointer input in logical (CSS-px) coordinates. `phase`: 0 down, 1 move,
/// 2 up.
pub fn session_pointer(phase: u32, x: f64, y: f64) {
    with_session(|hl| {
        let pe = PointerEvent::at(Point::new(x, y));
        hl.inject(match phase {
            0 => Event::PointerDown(pe),
            1 => Event::PointerMove(pe),
            _ => Event::PointerUp(pe),
        });
    });
}

/// Committed text (the browser's `keydown` with a single-char `key`, or an
/// `input` event on the hidden IME proxy).
pub fn session_text(text: &str) {
    if text.is_empty() {
        return;
    }
    with_session(|hl| {
        hl.inject(Event::TextInput(TextInputEvent {
            text: text.to_string(),
        }))
    });
}

/// Named key by a stable numeric code (shared with the JS glue):
/// 0 Enter, 1 Escape, 2 Backspace, 3 Delete, 4 Tab, 5 Space, 6..9 arrows
/// L/R/U/D, 10 Home, 11 End, 12 PageUp, 13 PageDown. `down` false = key-up.
pub fn session_key(code: u32, down: bool, shift: bool, ctrl: bool) {
    let named = match code {
        0 => NamedKey::Enter,
        1 => NamedKey::Escape,
        2 => NamedKey::Backspace,
        3 => NamedKey::Delete,
        4 => NamedKey::Tab,
        5 => NamedKey::Space,
        6 => NamedKey::ArrowLeft,
        7 => NamedKey::ArrowRight,
        8 => NamedKey::ArrowUp,
        9 => NamedKey::ArrowDown,
        10 => NamedKey::Home,
        11 => NamedKey::End,
        12 => NamedKey::PageUp,
        13 => NamedKey::PageDown,
        _ => return,
    };
    let mut mods = Modifiers::empty();
    if shift {
        mods |= Modifiers::SHIFT;
    }
    if ctrl {
        mods |= Modifiers::CTRL;
    }
    with_session(|hl| {
        let ke = KeyEvent {
            key: Key::Named(named),
            modifiers: mods,
            repeat: false,
        };
        hl.inject(if down {
            Event::KeyDown(ke)
        } else {
            Event::KeyUp(ke)
        });
    });
}

/// Wheel/scroll at a logical position.
pub fn session_wheel(x: f64, y: f64, dx: f64, dy: f64) {
    with_session(|hl| {
        hl.inject(Event::Wheel(WheelEvent {
            pos: Point::new(x, y),
            delta: lumen_core::geometry::Vec2::new(dx, dy),
            modifiers: Modifiers::empty(),
        }))
    });
}

/// One RAF tick: advance the virtual clock by `dt_ms`, pump, and — only when
/// the frame actually changed — write the physical-px RGBA frame into `out`.
/// Returns bytes written (0 ⇒ idle, keep the previous canvas contents).
pub fn session_frame(dt_ms: f64, out: &mut [u8]) -> usize {
    // M.5: drive queued wasm tasks (WasmSpawner) at RAF cadence — results
    // land through Sink like every other executor.
    #[cfg(target_arch = "wasm32")]
    lumen_core::tasks::pump_wasm_tasks();
    with_session(|hl| {
        hl.advance_clock(dt_ms.clamp(0.0, 1000.0));
        let stats = hl.pump();
        if !stats.painted {
            return 0;
        }
        let frame = hl.screenshot();
        let px = frame.pixels();
        if out.len() < px.len() {
            return 0;
        }
        out[..px.len()].copy_from_slice(px);
        px.len()
    })
    .unwrap_or(0)
}

/// Whether the UI asked for another frame (animations/wakes) — the RAF loop
/// idles when false and no input arrived.
pub fn session_needs_frame() -> bool {
    with_session(|hl| hl.next_deadline().is_some()).unwrap_or(false)
}

/// The current frame's physical pixel size (w, h) — canvas dimensions.
pub fn session_frame_size() -> (u32, u32) {
    with_session(|hl| {
        let s = hl.size();
        let k = hl.scale();
        (
            (s.width * k).round().max(1.0) as u32,
            (s.height * k).round().max(1.0) as u32,
        )
    })
    .unwrap_or((0, 0))
}

/// P.2 agent bridge: dispatch one JSON-RPC request line against the live
/// session — the same `lumen-agent` dispatch the desktop serves over TCP; the
/// JS host attaches whatever transport it likes (dev WebSocket, CDP).
pub fn session_agent(req: &str) -> String {
    with_session(|hl| {
        let v: serde_json::Value = serde_json::from_str(req).unwrap_or(serde_json::Value::Null);
        lumen_agent::dispatch(hl, &v).to_string()
    })
    .unwrap_or_else(|| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"no session"}}"#.into()
    })
}
