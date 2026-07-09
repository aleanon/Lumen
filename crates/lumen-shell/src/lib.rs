//! `lumen-shell` — the winit desktop shell (02 §8 `App::run`).
//!
//! Opens a window, drives the headless runtime each frame, and presents the
//! rendered frame to a wgpu surface. Input is translated to lumen [`Event`]s and
//! injected through the one input queue. Redraws are event-driven (idle ⇒ no
//! frames). Mobile shells arrive in M3.
//!
//! `App::run` is provided as an extension trait ([`RunExt`]) because `App` lives
//! in `lumen-widgets` (below this crate); the `lumen` facade re-exports it.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{
    Event, ImeEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent, PointerKind,
    TextInputEvent, WheelEvent,
};
use lumen_render::RgbaImage;
use lumen_widgets::{App, Headless};
#[cfg(feature = "agent")]
use std::io::{BufRead, Write};
#[cfg(feature = "agent")]
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

/// Extension trait adding `run()` to [`App`] (02 §8).
pub trait RunExt {
    /// Open a window and run the app to completion (blocks until close).
    fn run(self, size: Size);
}

impl RunExt for App {
    fn run(self, size: Size) {
        run(self, size);
    }
}

/// A message delivered into the winit event loop from a background thread —
/// currently just an agent JSON-RPC request awaiting a reply.
enum ShellEvent {
    /// One JSON-RPC request line; the response string is sent back on `reply`.
    #[cfg(feature = "agent")]
    Agent {
        req: String,
        reply: mpsc::Sender<String>,
    },
    /// New `.lss` source from the file-watcher (tier-1 hot reload, C1).
    ReloadStyles(String),
    /// A background task pushed a result; schedule a frame to apply it (the data
    /// layer waker target).
    Wake,
}

/// The shell's concrete runtime: CPU reference renderer + a real thread-pool
/// executor for the data layer.
// The live window renders through the dynamic-renderer seam (R = Box<dyn
// Renderer>), so the backend is chosen at startup: the GPU backend if an adapter
// is present, else the CPU reference. Both rasterize into the same Rgba8Unorm /
// sRGB-byte frame, which the presenter blits to the surface.
type ShellRenderer = Box<dyn lumen_widgets::Renderer>;
type ShellApp = App<ShellRenderer, lumen_core::tasks::ThreadPoolSpawner>;
type ShellHeadless = Headless<ShellRenderer, lumen_core::tasks::ThreadPoolSpawner>;

/// Open a window and run `app` at `size`.
///
/// If `LUMEN_AGENT_ADDR` is set (e.g. `127.0.0.1:9230`), a background thread
/// accepts newline-delimited JSON-RPC and forwards each request onto the event
/// loop, so an AI can observe (`ui.screenshot`/`ui.getTree`) and drive
/// (`input.click`/`type`/…) the **live** window over the agent protocol.
pub fn run(app: App, size: Size) {
    let event_loop = EventLoop::<ShellEvent>::with_user_event()
        .build()
        .expect("event loop");
    #[cfg(feature = "agent")]
    if let Some(addr) = std::env::var_os("LUMEN_AGENT_ADDR") {
        let addr = addr.to_string_lossy().into_owned();
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || serve_agent(&addr, proxy));
    }
    if let Some(path) = std::env::var_os("LUMEN_WATCH_LSS") {
        let path = path.to_string_lossy().into_owned();
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || watch_styles(&path, proxy));
    }
    // Upgrade the default inline executor to a real thread pool for the live app,
    // so `cx.resource`/`cx.task` run off the UI thread.
    let app = app.with_executor(lumen_core::tasks::ThreadPoolSpawner::default());
    // Choose the rasterization backend. An explicit `--wgpu` / `--tiny-skia` flag
    // or `LUMEN_RENDERER` env wins; otherwise the live window defaults to
    // GPU-with-CPU-fallback (paths, gradients, layers, text sprites rasterized on
    // the GPU when an adapter is present, else the CPU reference). R1.1.
    let renderer: ShellRenderer = lumen_widgets::renderer_override()
        .unwrap_or_else(|| Box::new(lumen_render::WgpuFallbackTinySkia::new()));
    eprintln!("lumen: renderer = {}", renderer.name());
    let app = app.with_renderer(renderer);
    let mut shell = Shell {
        app: Some(app),
        proxy: event_loop.create_proxy(),
        size,
        headless: None,
        window: None,
        presenter: None,
        direct: false,
        cursor: Point::ZERO,
        scale: 1.0,
        modifiers: Modifiers::empty(),
        ime_active: false,
        last_frame: Instant::now(),
        pending_resize: false,
        #[cfg(feature = "agent")]
        agent_session: lumen_agent::Session::new(),
    };
    event_loop.run_app(&mut shell).expect("run app");
}

/// Watch a `.lss` file and push its contents onto the event loop on every change
/// (and once at startup) for tier-1 desktop hot reload (C1).
fn watch_styles(path: &str, proxy: EventLoopProxy<ShellEvent>) {
    use notify::{RecursiveMode, Watcher};
    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("lumen watch: {e}");
            return;
        }
    };
    if watcher
        .watch(std::path::Path::new(path), RecursiveMode::NonRecursive)
        .is_err()
    {
        eprintln!("lumen watch: cannot watch {path}");
        return;
    }
    eprintln!("lumen watch: live-reloading {path}");
    let push = |proxy: &EventLoopProxy<ShellEvent>| {
        if let Ok(src) = std::fs::read_to_string(path) {
            let _ = proxy.send_event(ShellEvent::ReloadStyles(src));
        }
    };
    push(&proxy); // apply the current contents immediately
    for res in rx {
        if res.is_ok() {
            push(&proxy);
        }
    }
}

/// Accept agent connections and bridge each request line onto the event loop.
///
/// C.8a: `LUMEN_AGENT_ADDR=127.0.0.1:0` binds an ephemeral port (parallel
/// sessions never collide); the **bound** address is written to the discovery
/// file — `$LUMEN_AGENT_ADDR_FILE`, or `target/lumen-agent.addr` — which
/// `scripts/agent_client.py` reads automatically, and printed as a JSON ready
/// line on stderr.
#[cfg(feature = "agent")]
fn serve_agent(addr: &str, proxy: EventLoopProxy<ShellEvent>) {
    // C.5: a non-loopback bind exposes the app to the network — refuse it
    // unless a bearer token is configured (each request must then carry
    // `"auth": "<token>"`; `lumen agent call` attaches LUMEN_AGENT_TOKEN).
    let loopback =
        addr.starts_with("127.") || addr.starts_with("localhost:") || addr.starts_with("[::1]");
    if !loopback && std::env::var("LUMEN_AGENT_TOKEN").is_err() {
        eprintln!("lumen agent: refusing non-loopback bind {addr} without LUMEN_AGENT_TOKEN");
        return;
    }
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("lumen agent: cannot bind {addr}: {e}");
            return;
        }
    };
    let bound = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| addr.to_string());
    let discovery = std::env::var("LUMEN_AGENT_ADDR_FILE")
        .unwrap_or_else(|_| "target/lumen-agent.addr".to_string());
    if let Some(dir) = std::path::Path::new(&discovery).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&discovery, &bound) {
        eprintln!("lumen agent: cannot write discovery file {discovery}: {e}");
    }
    eprintln!("lumen agent: listening on {bound} (newline-delimited JSON-RPC)");
    eprintln!("{{\"lumen_agent_ready\":true,\"addr\":\"{bound}\",\"discovery\":\"{discovery}\"}}");
    for stream in listener.incoming().flatten() {
        let proxy = proxy.clone();
        std::thread::spawn(move || agent_conn(stream, proxy));
    }
}

/// Serve one connection: each line is a JSON-RPC request; reply with one line.
#[cfg(feature = "agent")]
fn agent_conn(stream: TcpStream, proxy: EventLoopProxy<ShellEvent>) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    for line in std::io::BufReader::new(read_half).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let (tx, rx) = mpsc::channel();
        if proxy
            .send_event(ShellEvent::Agent {
                req: line,
                reply: tx,
            })
            .is_err()
        {
            break; // event loop has exited
        }
        let Ok(resp) = rx.recv() else { break };
        if writeln!(writer, "{resp}").is_err() || writer.flush().is_err() {
            break;
        }
    }
}

struct Shell {
    app: Option<ShellApp>,
    /// Event-loop proxy used to build the data-layer waker (so background results
    /// schedule a frame).
    proxy: EventLoopProxy<ShellEvent>,
    size: Size,
    headless: Option<ShellHeadless>,
    window: Option<Arc<Window>>,
    /// CPU-readback presenter — only used as the fallback when the renderer can't
    /// present directly to the surface (`direct == false`). `None` in direct mode.
    presenter: Option<Presenter>,
    /// True when the renderer presents straight to the swapchain on its own device
    /// (1c): no second wgpu device, no GPU→CPU→GPU readback per frame.
    direct: bool,
    /// Pointer position in *logical* px (physical ÷ scale), the runtime's space.
    cursor: Point,
    /// HiDPI scale factor of the window.
    scale: f64,
    /// Current keyboard modifier state (Ctrl/Shift/Alt/Meta).
    modifiers: Modifiers,
    /// Whether an IME composition context is active (then text arrives via
    /// `Ime::Commit`, not `KeyEvent::text`).
    ime_active: bool,
    /// Wall-clock time of the previous presented frame; the delta drives the
    /// runtime's virtual clock. The shell is the *only* place wall time enters.
    last_frame: Instant,
    /// Set when a `Resized`/`ScaleFactorChanged` event has updated `size`/`scale`
    /// but the new frame hasn't been rendered yet. winit collapses the resize
    /// event storm into a single `RedrawRequested`, where we apply the resize and
    /// present exactly once — one GPU render per displayed frame, not per event.
    pending_resize: bool,
    /// C.3: agent requests route through a recording [`lumen_agent::Session`],
    /// so `session.assertText`/`assertState`/`exportTest` work against the
    /// **live** window — explore live, commit the exported regression test.
    #[cfg(feature = "agent")]
    agent_session: lumen_agent::Session,
}

impl ApplicationHandler<ShellEvent> for Shell {
    /// An agent request arrived from the server thread: dispatch it against the
    /// live runtime (same `dispatch` the headless agent uses), present any
    /// resulting frame so the window reflects the action, and reply.
    fn user_event(&mut self, _el: &ActiveEventLoop, event: ShellEvent) {
        match event {
            #[cfg(feature = "agent")]
            ShellEvent::Agent { req, reply } => {
                let resp = if let Some(h) = &mut self.headless {
                    let v = serde_json::from_str::<serde_json::Value>(&req)
                        .unwrap_or(serde_json::Value::Null);
                    // C.5: when a bearer token is configured, every request
                    // must carry it — checked before anything dispatches.
                    if let Ok(token) = std::env::var("LUMEN_AGENT_TOKEN") {
                        if v.get("auth").and_then(|a| a.as_str()) != Some(token.as_str()) {
                            let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
                            let _ = reply.send(
                                serde_json::json!({ "jsonrpc": "2.0", "id": id,
                                    "error": { "code": -32001,
                                               "message": "unauthorized: missing/invalid `auth` token" } })
                                .to_string(),
                            );
                            return;
                        }
                    }
                    // C.8a: `app.quit` is a *shell* method (only the event
                    // loop can exit) — reply, then shut down cleanly. No
                    // more pkill teardown.
                    if v.get("method").and_then(|m| m.as_str()) == Some("app.quit") {
                        let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
                        let _ = reply.send(
                            serde_json::json!({ "jsonrpc": "2.0", "id": id,
                                                "result": { "ok": true } })
                            .to_string(),
                        );
                        _el.exit();
                        return;
                    }
                    // C.3: route through the recording Session so the live
                    // window supports session.* (assert + exportTest).
                    self.agent_session.dispatch(h, &v).to_string()
                } else {
                    r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"app not ready"}}"#
                        .to_string()
                };
                // Reflect any state change the action caused in the window (works
                // for both the direct-present and CPU-fallback paths).
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                let _ = reply.send(resp);
            }
            ShellEvent::ReloadStyles(src) => {
                // Tier-1 hot reload: apply the new stylesheet live; a parse error
                // keeps the previous one and is reported (C1).
                if let Some(h) = &mut self.headless {
                    match h.set_stylesheet(&src) {
                        lumen_widgets::ReloadResult::Ok => eprintln!("lumen reload: ok"),
                        lumen_widgets::ReloadResult::Failed(d) => {
                            eprintln!("lumen reload: rejected ({} diagnostics)", d.len())
                        }
                    }
                    if let Some(p) = &mut self.presenter {
                        p.present(&h.screenshot());
                    }
                }
            }
            ShellEvent::Wake => {
                // A background result is queued; pump applies it (drains the
                // deferred-op queue) and we present the new frame.
                if let Some(h) = &mut self.headless {
                    h.pump();
                    if let Some(p) = &mut self.presenter {
                        p.present(&h.screenshot());
                    }
                }
            }
        }
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Lumen")
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.size.width,
                self.size.height,
            ));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        window.set_ime_allowed(true); // receive IME composition + commit
        let app = self.app.take().expect("app");
        // Runtime works in logical px; the surface is physical. Derive the
        // logical size from the surface's physical size and the scale factor so
        // layout is DPI-correct and the frame matches the surface 1:1 (crisp).
        self.scale = window.scale_factor();
        let phys = window.inner_size();
        self.size = Size::new(
            (phys.width.max(1) as f64 / self.scale).max(1.0),
            (phys.height.max(1) as f64 / self.scale).max(1.0),
        );
        let mut headless = app.run_headless(self.size);
        headless.set_scale(self.scale);
        // Direct-to-surface present on the renderer's own device (1c): one wgpu
        // device, no GPU→CPU→GPU readback per frame. Falls back to a CPU-readback
        // Presenter when the backend can't present (CPU renderer / unsupported
        // adapter).
        self.direct =
            headless.attach_surface(window.clone().into(), phys.width.max(1), phys.height.max(1));
        self.presenter = if self.direct {
            None
        } else {
            Some(Presenter::new(window.clone()))
        };
        eprintln!(
            "lumen: present = {}",
            if self.direct {
                "direct-to-surface"
            } else {
                "cpu-readback"
            }
        );
        // Wake the loop when a background task pushes a result, so it gets applied
        // and presented (the data-layer waker).
        let proxy = self.proxy.clone();
        headless.set_waker(std::sync::Arc::new(move || {
            let _ = proxy.send_event(ShellEvent::Wake);
        }));
        self.headless = Some(headless);
        window.request_redraw(); // paint the first frame
        self.window = Some(window);
        self.last_frame = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                let (w, h) = (s.width.max(1), s.height.max(1));
                self.size = Size::new(w as f64 / self.scale, h as f64 / self.scale);
                // Defer everything to RedrawRequested: a drag fires a storm of
                // Resized events, so coalescing the surface reconfigure + relayout
                // + present into one-per-frame avoids recreating the swapchain (and
                // re-laying-out) many times per displayed frame.
                self.pending_resize = true;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                // Defer the rescale (surface reconfigure + render) to
                // RedrawRequested (coalesced, same as Resized).
                self.pending_resize = true;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = map_modifiers(m.state());
            }
            WindowEvent::CursorMoved { position, .. } => {
                // winit reports physical px; the runtime works in logical px.
                self.cursor = Point::new(position.x / self.scale, position.y / self.scale);
                self.inject(Event::PointerMove(PointerEvent::at(self.cursor)));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pe = PointerEvent {
                    pos: self.cursor,
                    button: map_button(button),
                    pointer: PointerKind::Mouse,
                    modifiers: self.modifiers,
                    click_count: 1,
                };
                self.inject(if state == ElementState::Pressed {
                    Event::PointerDown(pe)
                } else {
                    Event::PointerUp(pe)
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // winit's convention is positive-y = wheel up (away from the
                // user); negate so the runtime's wheel delta means "scroll the
                // content toward its end" (wheel down → positive → list moves
                // down). Handlers and the agent's `input.scroll` all use that
                // natural sign.
                let d = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(x as f64 * 40.0, -(y as f64) * 40.0)
                    }
                    MouseScrollDelta::PixelDelta(p) => Vec2::new(p.x, -p.y),
                };
                self.inject(Event::Wheel(WheelEvent {
                    pos: self.cursor,
                    delta: d,
                    modifiers: self.modifiers,
                }));
            }
            WindowEvent::Ime(ime) => match ime {
                // `ime_active` means *composing a preedit* — not merely that IME
                // is enabled. Otherwise platforms that fire `Ime::Enabled` for
                // every focused field (e.g. X11) would suppress ordinary typing,
                // which arrives as `KeyEvent::text`, never as `Ime::Commit`.
                Ime::Enabled | Ime::Disabled => self.ime_active = false,
                Ime::Preedit(text, cursor) => {
                    self.ime_active = !text.is_empty();
                    self.inject(Event::ImePreedit(ImeEvent {
                        preedit: text,
                        cursor,
                    }));
                }
                Ime::Commit(text) => {
                    self.ime_active = false;
                    self.inject(Event::TextInput(TextInputEvent { text }));
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                // Direct (non-IME) text entry: when no IME context is composing,
                // the key's resolved text is the committed character(s).
                if event.state == ElementState::Pressed && !self.ime_active {
                    if let Some(t) = &event.text {
                        if !t.is_empty() && !t.chars().all(char::is_control) {
                            self.inject(Event::TextInput(TextInputEvent {
                                text: t.to_string(),
                            }));
                        }
                    }
                }
                if let Some(k) = map_key(&event.logical_key) {
                    let ke = KeyEvent {
                        key: k,
                        modifiers: self.modifiers,
                        repeat: event.repeat,
                    };
                    self.inject(if event.state == ElementState::Pressed {
                        Event::KeyDown(ke)
                    } else {
                        Event::KeyUp(ke)
                    });
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(h) = &mut self.headless {
                    let resized = std::mem::take(&mut self.pending_resize);
                    let now = Instant::now();
                    let elapsed_ms = (now - self.last_frame).as_secs_f64() * 1000.0;
                    self.last_frame = now;
                    // Advance the virtual clock by real elapsed time, then pump.
                    // Clamp the step so a sleep/background pause becomes one
                    // bounded jump rather than a long skip (since the UI renders
                    // as a function of now_ms(), there is no tick backlog to
                    // replay — just a single catch-up frame).
                    h.advance_clock(elapsed_ms.min(1000.0));
                    if resized {
                        // Apply the coalesced size/scale and reconfigure the
                        // surface exactly once for this frame, then let the single
                        // pump below render the new size (prepare_resize doesn't
                        // pump, so there's no redundant relayout).
                        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
                        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
                        if self.direct {
                            h.resize_surface(pw, ph);
                        } else if let Some(p) = &mut self.presenter {
                            p.resize(pw, ph);
                        }
                        h.prepare_resize(self.size, self.scale);
                    }
                    // Present only when the frame actually changed (R2): an idle
                    // tick repaints nothing, so the surface keeps its last frame.
                    let stats = h.pump();
                    if stats.painted || resized {
                        if self.direct {
                            // GPU → swapchain directly, no readback (1c).
                            h.present_to_surface();
                        } else if let Some(p) = &mut self.presenter {
                            let frame = h.screenshot();
                            p.present(&frame);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// A `WaitUntil` deadline elapsed: a one-shot wake (e.g. a delayed reveal)
    /// is due, so ask for the frame that will reflect it.
    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Decide how to wait for the next frame from what the UI asked for, so an
    /// idle UI costs zero frames while an animating one runs free.
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(h) = &self.headless else { return };
        match h.next_deadline() {
            // Idle: sleep until the next OS event (input/resize/close).
            None => el.set_control_flow(ControlFlow::Wait),
            // Continuous animation: keep producing frames back-to-back.
            Some(t) if t <= h.now_ms() => {
                el.set_control_flow(ControlFlow::Poll);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            // One-shot wake: sleep until the (virtual==real) deadline.
            Some(t) => {
                let dt = (t - h.now_ms()).max(0.0);
                el.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_secs_f64(dt / 1000.0),
                ));
            }
        }
    }
}

impl Shell {
    fn inject(&mut self, ev: Event) {
        if let Some(h) = &mut self.headless {
            h.inject(ev);
        }
        if let Some(w) = &self.window {
            w.request_redraw(); // event-driven: redraw only after input
        }
    }
}

fn map_modifiers(s: winit::keyboard::ModifiersState) -> Modifiers {
    let mut m = Modifiers::empty();
    if s.shift_key() {
        m |= Modifiers::SHIFT;
    }
    if s.control_key() {
        m |= Modifiers::CTRL;
    }
    if s.alt_key() {
        m |= Modifiers::ALT;
    }
    if s.super_key() {
        m |= Modifiers::META;
    }
    m
}

fn map_button(b: MouseButton) -> PointerButton {
    match b {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Other(n) => PointerButton::Other(n),
        _ => PointerButton::Left,
    }
}

fn map_key(k: &winit::keyboard::Key) -> Option<Key> {
    use winit::keyboard::{Key as WK, NamedKey as WNK};
    match k {
        WK::Named(WNK::Tab) => Some(Key::Named(NamedKey::Tab)),
        WK::Named(WNK::Enter) => Some(Key::Named(NamedKey::Enter)),
        WK::Named(WNK::Space) => Some(Key::Named(NamedKey::Space)),
        WK::Named(WNK::Escape) => Some(Key::Named(NamedKey::Escape)),
        WK::Named(WNK::Backspace) => Some(Key::Named(NamedKey::Backspace)),
        WK::Named(WNK::ArrowLeft) => Some(Key::Named(NamedKey::ArrowLeft)),
        WK::Named(WNK::ArrowRight) => Some(Key::Named(NamedKey::ArrowRight)),
        WK::Named(WNK::ArrowUp) => Some(Key::Named(NamedKey::ArrowUp)),
        WK::Named(WNK::ArrowDown) => Some(Key::Named(NamedKey::ArrowDown)),
        WK::Named(WNK::Home) => Some(Key::Named(NamedKey::Home)),
        WK::Named(WNK::End) => Some(Key::Named(NamedKey::End)),
        WK::Named(WNK::PageUp) => Some(Key::Named(NamedKey::PageUp)),
        WK::Named(WNK::PageDown) => Some(Key::Named(NamedKey::PageDown)),
        WK::Character(s) => Some(Key::Character(s.as_str().into())),
        _ => None,
    }
}

/// Presents a CPU-rendered frame to a wgpu surface via a fullscreen blit.
struct Presenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Cached blit texture + bind group, keyed by `(width, height)`. Recreated
    /// only when the frame size changes (on resize), not every present — so a
    /// steady stream of same-size frames just re-uploads pixels.
    staging: Option<(wgpu::Texture, wgpu::BindGroup, u32, u32)>,
}

impl Presenter {
    fn new(window: Arc<Window>) -> Presenter {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).expect("surface");
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("adapter");
        // MemoryUsage over the default Performance hint — the blit presenter holds
        // one small texture; no need for large pre-reserved GPU pools.
        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            },
            None,
        ))
        .expect("device");
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, // vsync
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Presenter {
            surface,
            device,
            queue,
            config,
            pipeline,
            bgl,
            sampler,
            staging: None,
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    fn present(&mut self, frame: &RgbaImage) {
        let (fw, fh) = (frame.width(), frame.height());
        // Reuse the blit texture + bind group across same-size frames; only
        // recreate them when the frame dimensions change (resize).
        if self.staging.as_ref().map(|(_, _, w, h)| (*w, *h)) != Some((fw, fh)) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame"),
                size: wgpu::Extent3d {
                    width: fw,
                    height: fh,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit-bg"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.staging = Some((tex, bind, fw, fh));
        }
        let (tex, bind, _, _) = self.staging.as_ref().unwrap();
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.pixels(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(fw * 4),
                rows_per_image: Some(fh),
            },
            wgpu::Extent3d {
                width: fw,
                height: fh,
                depth_or_array_layers: 1,
            },
        );

        // Reconfigure + retry once on a resize-outdated swapchain rather than
        // dropping the frame (smooth resize on the CPU-fallback path too).
        let surface_tex = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(_) => return,
                }
            }
            Err(_) => return,
        };
        let sview = surface_tex.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &sview,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
        surface_tex.present();
    }
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

const BLIT: &str = r#"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var uv = array<vec2<f32>, 3>(vec2<f32>(0.0,0.0), vec2<f32>(2.0,0.0), vec2<f32>(0.0,2.0));
    var o: VsOut;
    o.uv = uv[i];
    o.pos = vec4<f32>(uv[i] * 2.0 - 1.0, 0.0, 1.0);
    o.pos.y = -o.pos.y;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t, s, in.uv);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::ModifiersState;

    #[test]
    fn modifiers_map_to_lumen_flags() {
        assert_eq!(map_modifiers(ModifiersState::empty()), Modifiers::empty());
        assert_eq!(map_modifiers(ModifiersState::SHIFT), Modifiers::SHIFT);
        assert_eq!(
            map_modifiers(ModifiersState::CONTROL | ModifiersState::ALT),
            Modifiers::CTRL | Modifiers::ALT
        );
        let all = ModifiersState::SHIFT
            | ModifiersState::CONTROL
            | ModifiersState::ALT
            | ModifiersState::SUPER;
        assert_eq!(
            map_modifiers(all),
            Modifiers::SHIFT | Modifiers::CTRL | Modifiers::ALT | Modifiers::META
        );
    }
}
