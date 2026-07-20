//! Android-only implementation: native-activity event loop + software blit.
//!
//! P.1: input is wired — touch, keys (incl. unicode via `KeyCharacterMap`),
//! the back button, safe-area insets (content rect), and the soft keyboard —
//! all through the same one input queue headless tests use.

use android_activity::input::{InputEvent, KeyAction, Keycode, MotionAction};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use lumen::{App, BuildCx, Element, Headless};
use lumen_core::events::{
    Event, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent, PointerKind,
    TextInputEvent,
};
use lumen_core::geometry::{Point, Size};
use ndk::hardware_buffer_format::HardwareBufferFormat;
use std::rc::Rc;
use std::time::Duration;

/// Run `build` as a Lumen app on this `NativeActivity`, presenting CPU frames to
/// the window. Returns when the activity is destroyed.
pub fn run(android: AndroidApp, build: impl Fn(&mut BuildCx) -> Element + 'static) {
    run_styled(android, build, None)
}

/// Like [`run`], but applies an initial `.lss` stylesheet (e.g. an app's bundled
/// theme). Tier-1 reloads still override it from the watched file.
pub fn run_styled(
    android: AndroidApp,
    build: impl Fn(&mut BuildCx) -> Element + 'static,
    initial_lss: Option<&str>,
) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("lumen android shell starting");

    let initial_lss = initial_lss.map(|s| s.to_string());
    let build = Rc::new(build);
    let mut headless: Option<Headless> = None;
    // Content-rect layout box (safe area): physical size, window-space
    // origin, and the DPI scale (density/160 — layout runs in logical px).
    let mut content = Viewport::default();
    let mut quit = false;
    let mut kb_shown = false;

    // Tier-1 hot reload (T3.2): the dev orchestration pushes the live stylesheet
    // into the app's external files dir (adb-writable, app-readable — unlike
    // /data/local/tmp which SELinux denies the app); the shell re-applies it
    // whenever it changes.
    let lss_path = std::env::var("LUMEN_LSS_PATH").ok().unwrap_or_else(|| {
        android
            .external_data_path()
            .map(|p| p.join("lumen.lss").to_string_lossy().into_owned())
            .unwrap_or_else(|| "/data/local/tmp/lumen.lss".to_string())
    });
    log::info!("tier-1 stylesheet path: {lss_path}");
    let mut lss_mtime: Option<std::time::SystemTime> = None;

    while !quit {
        android.poll_events(Some(Duration::from_millis(250)), |event| match event {
            PollEvent::Main(MainEvent::InitWindow { .. })
            | PollEvent::Main(MainEvent::RedrawNeeded { .. })
            | PollEvent::Main(MainEvent::WindowResized { .. })
            | PollEvent::Main(MainEvent::ContentRectChanged { .. }) => {
                present(
                    &android,
                    &build,
                    initial_lss.as_deref(),
                    &mut headless,
                    &mut content,
                );
            }
            PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
                // Surface is gone (background/rotation). Keep the app alive —
                // the signals survive; only presentation stops.
                content = Viewport::default();
            }
            PollEvent::Main(MainEvent::Destroy) => quit = true,
            _ => {}
        });

        // P.1: drain OS input into the one input queue, then pump + present.
        if let Some(hl) = headless.as_mut() {
            if drain_input(&android, hl, &content) {
                sync_soft_keyboard(&android, hl, &mut kb_shown);
                present(
                    &android,
                    &build,
                    initial_lss.as_deref(),
                    &mut headless,
                    &mut content,
                );
            }
        }

        // Poll the stylesheet file each tick (~4 Hz via the poll timeout).
        let changed = std::fs::metadata(&lss_path)
            .and_then(|m| m.modified())
            .ok()
            .filter(|mt| lss_mtime != Some(*mt));
        if let Some(mt) = changed {
            if let (Ok(src), Some(hl)) = (std::fs::read_to_string(&lss_path), headless.as_mut()) {
                lss_mtime = Some(mt);
                log::info!("tier-1 reload: {} bytes of .lss", src.len());
                let _ = hl.set_stylesheet(&src);
                present(
                    &android,
                    &build,
                    initial_lss.as_deref(),
                    &mut headless,
                    &mut content,
                );
            }
        }
    }
}

/// Drain the input queue into the runtime; returns whether anything was
/// injected (⇒ pump + present). Touch becomes the standard pointer trio; the
/// back button becomes Escape (dismisses overlays — app exit stays an app
/// decision); other keys map to named keys or unicode text through the
/// device's `KeyCharacterMap` (physical/emulator keyboards; true IME commit
/// text needs GameActivity and is out of scope for the native-activity shell).
fn drain_input(android: &AndroidApp, hl: &mut Headless, vp: &Viewport) -> bool {
    let mut changed = false;
    let Ok(mut iter) = android.input_events_iter() else {
        return false;
    };
    loop {
        let read = iter.next(|ev| {
            match ev {
                InputEvent::MotionEvent(m) => {
                    let idx = m.pointer_index();
                    let p = m.pointer_at_index(idx);
                    // Window-physical → content-logical (the runtime's space).
                    let pos = Point::new(
                        (f64::from(p.x()) - f64::from(vp.l)) / vp.scale,
                        (f64::from(p.y()) - f64::from(vp.t)) / vp.scale,
                    );
                    let pe = PointerEvent {
                        pos,
                        button: PointerButton::Left,
                        pointer: PointerKind::Touch,
                        modifiers: Modifiers::empty(),
                        click_count: 1,
                    };
                    match m.action() {
                        MotionAction::Down | MotionAction::PointerDown => {
                            hl.inject(Event::PointerDown(pe));
                            changed = true;
                        }
                        MotionAction::Move => {
                            hl.inject(Event::PointerMove(pe));
                            changed = true;
                        }
                        MotionAction::Up | MotionAction::PointerUp | MotionAction::Cancel => {
                            hl.inject(Event::PointerUp(pe));
                            changed = true;
                        }
                        _ => {}
                    }
                    InputStatus::Handled
                }
                InputEvent::KeyEvent(k) => {
                    let down = k.action() == KeyAction::Down;
                    let named = match k.key_code() {
                        // Back = Escape: closes overlays/sheets through the
                        // standard dismiss path; the OS keeps Home/Recents.
                        Keycode::Back | Keycode::Escape => Some(NamedKey::Escape),
                        Keycode::Enter | Keycode::NumpadEnter => Some(NamedKey::Enter),
                        Keycode::Del => Some(NamedKey::Backspace),
                        Keycode::ForwardDel => Some(NamedKey::Delete),
                        Keycode::Tab => Some(NamedKey::Tab),
                        Keycode::DpadLeft => Some(NamedKey::ArrowLeft),
                        Keycode::DpadRight => Some(NamedKey::ArrowRight),
                        Keycode::DpadUp => Some(NamedKey::ArrowUp),
                        Keycode::DpadDown => Some(NamedKey::ArrowDown),
                        Keycode::MoveHome => Some(NamedKey::Home),
                        Keycode::MoveEnd => Some(NamedKey::End),
                        _ => None,
                    };
                    if let Some(nk) = named {
                        let ke = KeyEvent {
                            key: Key::Named(nk),
                            modifiers: Modifiers::empty(),
                            repeat: false,
                        };
                        hl.inject(if down {
                            Event::KeyDown(ke)
                        } else {
                            Event::KeyUp(ke)
                        });
                        changed = true;
                        return InputStatus::Handled;
                    }
                    // Printable keys: resolve unicode via the device key map.
                    if down {
                        if let Ok(map) = android.device_key_character_map(k.device_id()) {
                            if let Ok(android_activity::input::KeyMapChar::Unicode(ch)) =
                                map.get(k.key_code(), k.meta_state())
                            {
                                if !ch.is_control() {
                                    hl.inject(Event::TextInput(TextInputEvent {
                                        text: ch.to_string(),
                                    }));
                                    changed = true;
                                    return InputStatus::Handled;
                                }
                            }
                        }
                    }
                    InputStatus::Unhandled
                }
                // TextEvent/TextAction are GameActivity-backend IME events;
                // the native-activity backend never delivers them.
                _ => InputStatus::Unhandled,
            }
        });
        if !read {
            break;
        }
    }
    changed
}

/// Show the soft keyboard while a text input holds focus, hide it after
/// (checked after each input-driven pump; the content rect shrinks when the
/// IME appears, so layout adapts through the normal resize path).
fn sync_soft_keyboard(android: &AndroidApp, hl: &Headless, shown: &mut bool) {
    fn focused_text(n: &lumen_core::semantics::SemanticsNode) -> bool {
        let hit = n.role == lumen_core::semantics::Role::TextInput
            && n.states
                .iter()
                .any(|s| matches!(s, lumen_core::semantics::State::Focused));
        hit || n.children.iter().any(focused_text)
    }
    let want = focused_text(&hl.semantics_doc().root);
    if want != *shown {
        *shown = want;
        if want {
            android.show_soft_input(true);
        } else {
            android.hide_soft_input(true);
        }
    }
}

/// The realized viewport: content-rect physical box + DPI scale.
#[derive(Clone, Copy, PartialEq)]
struct Viewport {
    w: u32,
    h: u32,
    l: i32,
    t: i32,
    scale: f64,
}

impl Default for Viewport {
    fn default() -> Viewport {
        Viewport {
            w: 0,
            h: 0,
            l: 0,
            t: 0,
            scale: 1.0,
        }
    }
}

fn present(
    android: &AndroidApp,
    build: &Rc<impl Fn(&mut BuildCx) -> Element + 'static>,
    initial_lss: Option<&str>,
    headless: &mut Option<Headless>,
    content: &mut Viewport,
) {
    let Some(window) = android.native_window() else {
        return;
    };
    let (win_w, win_h) = (window.width() as u32, window.height() as u32);
    if win_w == 0 || win_h == 0 {
        return;
    }
    // Safe area (P.1): lay out inside the content rect — the OS-reported box
    // that excludes status/navigation bars and shrinks when the IME shows.
    let rect = android.content_rect();
    let (mut l, mut t) = (rect.left.max(0), rect.top.max(0));
    let mut w = (rect.right - rect.left).max(0) as u32;
    let mut h = (rect.bottom - rect.top).max(0) as u32;
    if w == 0 || h == 0 || w > win_w || h > win_h {
        (l, t, w, h) = (0, 0, win_w, win_h);
    }
    // DPI scale: layout in logical px (density/160), render at physical.
    let scale = android
        .config()
        .density()
        .map(|d| f64::from(d) / 160.0)
        .unwrap_or(1.0)
        .max(0.5);
    let vp = Viewport { w, h, l, t, scale };
    let logical = Size::new(w as f64 / scale, h as f64 / scale);

    // Boot once; resize in place afterwards (state survives — P.1 replaced
    // the old rebuild-on-resize, which silently dropped every signal).
    if headless.is_none() {
        let build = build.clone();
        let mut app = App::new(move |cx| (build)(cx));
        if let Some(lss) = initial_lss {
            app = app.stylesheet(lss);
        }
        let mut hl = app.run_headless(logical);
        hl.set_scale(scale);
        *headless = Some(hl);
    }
    let hl = headless.as_mut().unwrap();
    if *content != vp {
        *content = vp;
        hl.prepare_resize(logical, scale);
    }
    hl.pump();
    let frame = hl.screenshot();

    if window
        .set_buffers_geometry(
            win_w as i32,
            win_h as i32,
            Some(HardwareBufferFormat::R8G8B8A8_UNORM),
        )
        .is_err()
    {
        log::warn!("set_buffers_geometry failed");
        return;
    }
    let Ok(mut buf) = window.lock(None) else {
        log::warn!("native window lock failed");
        return;
    };

    // Clear, then blit the frame into the content rect, honouring the
    // buffer stride. The clear matters: buffers rotate (double/triple
    // buffering) and the content offset can change (IME/system bars), so a
    // stale frame at the old offset would ghost through otherwise.
    let stride = buf.stride(); // pixels per row
    let buf_w = buf.width();
    let buf_h = buf.height();
    let src = frame.pixels();
    let (fw, fh) = (frame.width() as usize, frame.height() as usize);
    let dst = buf.bits() as *mut u8;
    // SAFETY: the locked buffer is stride×height RGBA8.
    unsafe {
        std::ptr::write_bytes(dst, 0, stride * buf_h * 4);
    }
    let copy_w = fw.min(buf_w.saturating_sub(l as usize));
    for y in 0..fh.min(buf_h.saturating_sub(t as usize)) {
        let s = y * fw * 4;
        let d = ((y + t as usize) * stride + l as usize) * 4;
        // SAFETY: both ranges are within their buffers (copy_w bounds both).
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr().add(s), dst.add(d), copy_w * 4);
        }
    }
}
