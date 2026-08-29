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
    DropData, DropEvent, Event, ImeEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton,
    PointerEvent, PointerKind, TextInputEvent, WheelEvent,
};
use lumen_render::RgbaImage;
#[cfg(feature = "wgpu")]
use lumen_widgets::Present;
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

/// MOD7 S1/S4: implemented for an app on ANY renderer, executor and
/// [`PlatformConfig`] — not just the shipped bundle, and not just the default
/// renderer and executor.
///
/// The wider bound is what makes the presets usable. `presets::Desktop` names a
/// `Box<dyn Renderer>` and a thread pool, so with the original
/// `App<TinySkia, InlineSpawner, P>` bound a preset app could run headless and
/// **not** open a window — the same "reachable everywhere except the shell"
/// defect MOD7 exists to remove, reintroduced one layer up.
///
/// The executor is kept; the renderer is not — see [`run_any`].
impl<
        R: lumen_render::Renderer,
        E: lumen_core::tasks::Spawner,
        P: lumen_widgets::app::PlatformConfig,
    > RunExt for App<R, E, P>
{
    fn run(self, size: Size) {
        run_any(self, size);
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
    /// P.4: an assistive technology activated, deactivated, or requested an
    /// action; the accesskit_winit adapter posts these through the loop.
    #[cfg(feature = "accessibility")]
    AccessKit(accesskit_winit::Event),
    /// P.3c/P.3e: a native menu activation — menubar item (Windows/macOS) or
    /// tray-menu item (all platforms; the tray menu hosts the app's
    /// `MenuModel`). Pushed via muda's event handler so the click *wakes*
    /// the loop (a drain in `about_to_wait` only ran on the next unrelated
    /// event).
    #[cfg(feature = "desktop-integration")]
    Menu(muda::MenuEvent),
}

#[cfg(feature = "accessibility")]
impl From<accesskit_winit::Event> for ShellEvent {
    fn from(ev: accesskit_winit::Event) -> ShellEvent {
        ShellEvent::AccessKit(ev)
    }
}

/// The shell's concrete runtime: CPU reference renderer + a real thread-pool
/// executor for the data layer.
// The live window renders through the dynamic-renderer seam (R = Box<dyn
// Renderer>), so the backend is chosen at startup: the GPU backend if an adapter
// is present, else the CPU reference. Both rasterize into the same Rgba8Unorm /
// sRGB-byte frame, which the presenter blits to the surface.
type ShellRenderer = Box<dyn lumen_widgets::Renderer>;
// MOD7 S1: generic over the platform bundle. These were fixed at
// `DefaultPlatform`, which is what made every seam headless-only — a windowed
// app could not select a layout or text engine no matter what it passed to
// `with_platform`. Defaulted, so every existing `run(app, size)` is unchanged.
type ShellApp<E = lumen_core::tasks::ThreadPoolSpawner, P = lumen_widgets::app::DefaultPlatform> =
    App<ShellRenderer, E, P>;
type ShellHeadless<
    E = lumen_core::tasks::ThreadPoolSpawner,
    P = lumen_widgets::app::DefaultPlatform,
> = Headless<ShellRenderer, E, P>;

/// Open a window and run `app` at `size`.
///
/// If `LUMEN_AGENT_ADDR` is set (e.g. `127.0.0.1:9230`), a background thread
/// accepts newline-delimited JSON-RPC and forwards each request onto the event
/// loop, so an AI can observe (`ui.screenshot`/`ui.getTree`) and drive
/// (`input.click`/`type`/…) the **live** window over the agent protocol.
pub fn run<P: lumen_widgets::app::PlatformConfig>(
    app: App<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner, P>,
    size: Size,
) {
    // Upgrade the default inline executor to a real thread pool for the live
    // app, so `cx.resource`/`cx.task` run off the UI thread. An app that has
    // already chosen an executor goes through `run_with` instead and keeps it.
    run_with(
        app.with_executor(lumen_core::tasks::ThreadPoolSpawner::default()),
        size,
    )
}

/// MOD7 S2: open a window and run `app` on **the executor it already carries**.
///
/// [`run`] upgrades the default `InlineSpawner` to a thread pool, which is the
/// right default and was previously unconditional — the shell overwrote
/// whatever the caller had chosen, so a windowed app could not run on tokio or
/// smol (`lumen-exec`) no matter what it passed to `with_executor`. This is the
/// entry point that honours it; `run` now delegates here.
pub fn run_with<E: lumen_core::tasks::Spawner, P: lumen_widgets::app::PlatformConfig>(
    app: App<lumen_render::TinySkia, E, P>,
    size: Size,
) {
    run_any(app, size)
}

/// Open a window and run `app`, whatever renderer and executor it carries.
///
/// **The renderer is replaced.** A live window's backend is chosen at startup —
/// GPU if an adapter is present, else the CPU reference, with `--wgpu` /
/// `--tiny-skia` / `LUMEN_RENDERER` overriding — and that choice cannot be
/// expressed by a type the app picked earlier. So a config's `Renderer` governs
/// headless runs and is discarded here. Stated rather than left to be
/// discovered, because silently dropping a caller's type parameter is precisely
/// the defect MOD7 S0 had to fix.
///
/// The executor IS kept, unlike before MOD7 S2.
pub fn run_any<
    R: lumen_render::Renderer,
    E: lumen_core::tasks::Spawner,
    P: lumen_widgets::app::PlatformConfig,
>(
    app: App<R, E, P>,
    size: Size,
) {
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
    // P.3c/P.3e: native menu + tray-menu clicks land here from muda's
    // handler thread; forward them into the loop (each one wakes it).
    #[cfg(feature = "desktop-integration")]
    {
        let proxy = event_loop.create_proxy();
        muda::MenuEvent::set_event_handler(Some(move |ev: muda::MenuEvent| {
            let _ = proxy.send_event(ShellEvent::Menu(ev));
        }));
    }
    // Choose the rasterization backend. An explicit `--wgpu` / `--tiny-skia` flag
    // or `LUMEN_RENDERER` env wins; otherwise the live window defaults to
    // GPU-with-CPU-fallback (paths, gradients, layers, text sprites rasterized on
    // the GPU when an adapter is present, else the CPU reference). R1.1.
    // Without the GPU backend compiled there is nothing to fall back FROM, so
    // the CPU reference renderer is the only default (ADR-003 amendment).
    #[cfg(feature = "wgpu")]
    let renderer: ShellRenderer = lumen_widgets::renderer_override()
        .unwrap_or_else(|| Box::new(lumen_render::WgpuFallbackTinySkia::new()));
    #[cfg(not(feature = "wgpu"))]
    let renderer: ShellRenderer =
        lumen_widgets::renderer_override().unwrap_or_else(|| Box::new(lumen_render::TinySkia));
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
        #[cfg(feature = "desktop-integration")]
        menu_rev_seen: 0,
        #[cfg(feature = "desktop-integration")]
        native_menu: None,
        #[cfg(feature = "accessibility")]
        a11y: None,
        force_present: false,
        #[cfg(feature = "desktop-integration")]
        tray: None,
        cursor_shape: None,
        secondary: std::collections::HashMap::new(),
        os_clipboard: arboard::Clipboard::new().ok(),
        os_clip_last: String::new(),
        ime_active: false,
        last_frame: Instant::now(),
        pending_resize: false,
        #[cfg(feature = "wgpu")]
        skipped_presents: 0,
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
/// Is `addr` a loopback bind target?
///
/// C.5's fail-closed guard: a non-loopback bind exposes the app's full remote
/// control surface, so it is refused unless a bearer token is configured.
///
/// The original check was `starts_with("127.") || starts_with("localhost:") ||
/// starts_with("[::1]")`, which a hostname defeats: `127.0.0.1.attacker.example:9000`
/// begins with `127.` yet resolves wherever its DNS says, so the guard passed
/// and the socket went public tokenless. Parse instead — an IP literal is the
/// only thing that can be judged loopback without resolving, and `localhost`
/// is the one name defined to be it.
///
/// Not gated on `agent`: it must be compiled, and tested, in the default
/// profile the CI test job actually runs.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
fn is_loopback_addr(addr: &str) -> bool {
    // Env vars sourced from a file commonly carry a trailing newline, and the
    // parsers below are strict; without this the guard fails CLOSED on a
    // perfectly good loopback address and the endpoint silently never starts.
    let addr = addr.trim();
    if let Ok(sa) = addr.parse::<std::net::SocketAddr>() {
        return sa.ip().is_loopback();
    }
    if let Ok(ip) = addr.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    // A name, not a literal. Only exactly "localhost" (with a real port)
    // qualifies; `rsplit_once` so an IPv6-looking name cannot smuggle a colon.
    match addr.rsplit_once(':') {
        Some((host, port)) => port.parse::<u16>().is_ok() && host.eq_ignore_ascii_case("localhost"),
        None => addr.eq_ignore_ascii_case("localhost"),
    }
}

/// Normalise a raw `LUMEN_AGENT_TOKEN` value into "is a token configured?".
///
/// An EMPTY value counts as UNSET. `LUMEN_AGENT_TOKEN=` yields `Ok("")` from
/// `env::var`, so testing it with `is_err()` treated it as configured: the
/// non-loopback refusal was skipped, and `auth_ok(Some(""), Some(""))` then
/// accepted every request — publishing exactly the tokenless remote-control
/// socket the guard exists to prevent. Both call sites go through here so the
/// two cannot disagree again.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
fn normalize_token(raw: Option<String>) -> Option<String> {
    raw.filter(|t| !t.trim().is_empty())
}

/// The configured bearer token, or `None` if none is set.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
fn configured_token() -> Option<String> {
    normalize_token(std::env::var("LUMEN_AGENT_TOKEN").ok())
}

/// Does a request carry the configured bearer token?
///
/// `token == None` means none is configured, which is only reachable on a
/// loopback bind (see [`is_loopback_addr`]) and is therefore open by design.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
fn auth_ok(provided: Option<&str>, token: Option<&str>) -> bool {
    let Some(t) = token else { return true };
    let Some(p) = provided else { return false };
    // Constant-time comparison. `==` on `&str` stops at the first differing
    // byte, and this gates a socket granting full remote control. A TCP round
    // trip plus the hop onto the winit event loop buries a few nanoseconds of
    // prefix difference in jitter, so this is closing a question rather than a
    // demonstrated hole — but it is three lines. Length is not hidden; that is
    // standard for bearer tokens.
    let (p, t) = (p.as_bytes(), t.as_bytes());
    if p.len() != t.len() {
        return false;
    }
    p.iter().zip(t).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(feature = "agent")]
fn serve_agent(addr: &str, proxy: EventLoopProxy<ShellEvent>) {
    // C.5: a non-loopback bind exposes the app to the network — refuse it
    // unless a bearer token is configured (each request must then carry
    // `"auth": "<token>"`; `lumen agent call` attaches LUMEN_AGENT_TOKEN).
    if !is_loopback_addr(addr) && configured_token().is_none() {
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

struct Shell<
    E: lumen_core::tasks::Spawner = lumen_core::tasks::ThreadPoolSpawner,
    P: lumen_widgets::app::PlatformConfig = lumen_widgets::app::DefaultPlatform,
> {
    app: Option<ShellApp<E, P>>,
    /// Event-loop proxy used to build the data-layer waker (so background results
    /// schedule a frame).
    proxy: EventLoopProxy<ShellEvent>,
    size: Size,
    headless: Option<ShellHeadless<E, P>>,
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
    /// P.3a: the OS clipboard (arboard), bridged to the portable Runtime
    /// clipboard — `None` when unavailable (no display server). Pull before
    /// Ctrl-modified keys (paste path); push after a pump when the app-side
    /// text changed (copy path).
    os_clipboard: Option<arboard::Clipboard>,
    /// The last text pushed to / pulled from the OS clipboard, to avoid
    /// redundant round-trips.
    os_clip_last: String,
    /// Current keyboard modifier state (Ctrl/Shift/Alt/Meta).
    modifiers: Modifiers,
    /// P.3c: [`Headless::menu_rev`] last realized as a native (muda) menu —
    /// the menu is rebuilt only when the app installs a new model.
    #[cfg(feature = "desktop-integration")]
    menu_rev_seen: u64,
    /// The realized native menu. Attached to the window on Windows/macOS;
    /// on Linux muda is GTK-bound and winit offers no menubar attachment
    /// point, so the model stays data and accelerators/agent verbs activate
    /// items (see `attach_native_menu`).
    #[cfg(feature = "desktop-integration")]
    native_menu: Option<muda::Menu>,
    /// P.4: the AccessKit adapter. The *tree* is dormant until an assistive
    /// technology subscribes — then the semantic tree, the same one the agent
    /// and tests read, is published after every frame. The adapter itself is
    /// NOT free: constructing it spawns a D-Bus thread on Linux whether or not
    /// an AT exists, which is why creation is gated by `a11y_enabled` (GX4).
    #[cfg(feature = "accessibility")]
    a11y: Option<accesskit_winit::Adapter>,
    /// Present on the next `RedrawRequested` even if its pump paints nothing:
    /// set by paths that already pumped (agent dispatch, AT actions, style
    /// reload) so their frame reaches the surface. Without it, the pre-pump
    /// consumes the damage and the redraw's painted-check skips the present —
    /// on the direct-to-surface path the glass then never updates (hit live:
    /// a completed login kept showing the stale login frame until real input
    /// arrived).
    force_present: bool,
    /// P.3d-2: realized secondary windows, keyed by their winit id. Each is
    /// an independent `Headless` pipeline over the shared `Runtime`
    /// (`Headless::open_window_with`); input routes here by window id, and
    /// any injected event schedules a redraw of *every* window (shared
    /// signals may change any of them; an untouched window's pump is a
    /// dirty-checked no-op).
    secondary: std::collections::HashMap<WindowId, SecondaryWindow<P>>,
    /// P.3e: system tray, created lazily on the app's first
    /// `SystemRequest::TrayTooltip` (no tray unless asked for). On Linux it
    /// lives on a dedicated gtk thread (winit owns the main loop) reached by
    /// this channel; elsewhere the TrayIcon is held directly.
    #[cfg(feature = "desktop-integration")]
    tray: Option<TrayState>,
    /// PROP1: the cursor SHAPE currently applied (distinct from `cursor`, which
    /// is the pointer position), so a frame only calls the platform when the
    /// shape actually changes.
    cursor_shape: Option<lumen_core::CursorShape>,
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
    /// O5.3: how many presents have been skipped this session. Only the
    /// direct-to-surface path can skip, so this exists only where that does.
    #[cfg(feature = "wgpu")]
    skipped_presents: u64,
    /// C.3: agent requests route through a recording [`lumen_agent::Session`],
    /// so `session.assertText`/`assertState`/`exportTest` work against the
    /// **live** window — explore live, commit the exported regression test.
    #[cfg(feature = "agent")]
    agent_session: lumen_agent::Session,
}

/// P.3d-2: one realized secondary window — its pipeline plus per-window
/// presentation state (mirrors the main-window fields on [`Shell`]).
/// MOD7 S2 note: a secondary window keeps the shell's own single-thread pool
/// rather than the main app's executor. `open_window_with` takes the executor
/// by value and `Spawner` is not `Clone`, so sharing the caller's would mean
/// either a `Clone` bound on the seam or an `Arc` indirection on every spawn —
/// more than a second window warrants. The main window honours the caller's
/// choice; this one does not, and that is a limitation rather than a design.
struct SecondaryWindow<P: lumen_widgets::app::PlatformConfig = lumen_widgets::app::DefaultPlatform>
{
    headless: ShellHeadless<lumen_core::tasks::ThreadPoolSpawner, P>,
    window: Arc<Window>,
    presenter: Option<Presenter>,
    /// Only read when a swapchain exists; without the GPU backend the
    /// softbuffer presenter is the only path and this is always false.
    #[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
    direct: bool,
    size: Size,
    scale: f64,
    cursor: Point,
    last_frame: Instant,
    pending_resize: bool,
}

impl<E: lumen_core::tasks::Spawner, P: lumen_widgets::app::PlatformConfig>
    ApplicationHandler<ShellEvent> for Shell<E, P>
{
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
                    let token = configured_token();
                    {
                        if !auth_ok(v.get("auth").and_then(|a| a.as_str()), token.as_deref()) {
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
                // Reflect any state change the action caused in the window(s) —
                // shared signals may re-render secondaries too (P.3d-2). The
                // dispatch already pumped, so force the present.
                self.force_present = true;
                self.redraw_all();
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
                    self.force_present = true;
                    self.redraw_all();
                }
            }
            ShellEvent::Wake => {
                // A background result is queued. Do NOT pump here: the redraw's
                // own pump must observe the damage, or its painted-check skips
                // the present and the frame never reaches the surface (the
                // presenter-only present this arm used to do was a no-op on the
                // direct-to-surface path).
                self.redraw_all();
            }
            #[cfg(feature = "desktop-integration")]
            ShellEvent::Menu(ev) => {
                if let Some(h) = &mut self.headless {
                    h.activate_menu(ev.id().0.as_str());
                }
                self.force_present = true;
                self.redraw_all();
            }
            #[cfg(feature = "accessibility")]
            ShellEvent::AccessKit(ev) => {
                use accesskit_winit::WindowEvent as AkEvent;
                match ev.window_event {
                    // An AT subscribed: publish the current tree.
                    AkEvent::InitialTreeRequested => self.push_a11y_tree(),
                    // AT action → the same input queue everything else uses.
                    AkEvent::ActionRequested(req) => {
                        if let Some(h) = &mut self.headless {
                            route_at_action(h, &req);
                        }
                        // route_at_action pumped; force the present.
                        self.force_present = true;
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    AkEvent::AccessibilityDeactivated => {}
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
            ))
            // P.4: the AccessKit adapter must exist before the window is
            // first shown — create invisible, attach, then show.
            .with_visible(false);
        let window = Arc::new(el.create_window(attrs).expect("window"));
        #[cfg(feature = "accessibility")]
        if a11y_enabled() {
            // accesskit_winit 0.33 takes the `ActiveEventLoop` too — it needs it
            // to register the adapter's handlers with the platform before the
            // window is shown.
            self.a11y = Some(accesskit_winit::Adapter::with_event_loop_proxy(
                el,
                &window,
                self.proxy.clone(),
            ));
        }
        window.set_visible(true);
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
        // Without the GPU backend compiled there is no swapchain to present to,
        // so the softbuffer `Presenter` is the only path (ADR-003 amendment).
        #[cfg(feature = "wgpu")]
        {
            self.direct = headless.attach_surface(
                window.clone().into(),
                phys.width.max(1),
                phys.height.max(1),
            );
        }
        #[cfg(not(feature = "wgpu"))]
        {
            self.direct = false;
        }
        self.presenter = if self.direct {
            None
        } else {
            Presenter::new(window.clone())
        };
        let mode = if self.direct {
            "direct-to-surface"
        } else {
            "cpu-readback"
        };
        eprintln!("lumen: present = {mode}");
        // O5.3: also into the ring. Under `just run-agent` the agent reads a
        // socket; stderr goes to the developer's terminal, so every fact below
        // was invisible to the thing that most needs it.
        //
        // The renderer identity is emitted HERE rather than beside its own
        // `eprintln!` at shell startup: that site runs on the pre-`Headless`
        // `App` builder, which owns no `Runtime` yet.
        headless.runtime().log(
            "info",
            format!(
                "renderer = {} (gpu: {}, backend: {}), present = {mode}",
                headless.renderer_name(),
                headless.is_gpu(),
                headless.backend()
            ),
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
        // P.3d-2: realize every declared secondary window.
        let descs: Vec<lumen_widgets::system::WindowDesc> = self
            .headless
            .as_ref()
            .map(|h| h.windows().to_vec())
            .unwrap_or_default();
        for d in &descs {
            self.open_secondary(el, d);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // P.3d-2: secondary windows have their own (reduced) event path —
        // pointer/keys/resize/redraw/close. Menus, accelerators, IME,
        // clipboard bridging, and the AT adapter stay main-window (the
        // adapter is bound to the main window's handle).
        if self.secondary.contains_key(&id) {
            self.secondary_event(el, id, event);
            return;
        }
        // P.4: the adapter tracks focus/visibility from the raw event stream.
        #[cfg(feature = "accessibility")]
        if let (Some(a), Some(w)) = (&mut self.a11y, &self.window) {
            a.process_event(w, &event);
        }
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
                    MouseScrollDelta::LineDelta(x, y) => Vec2::new(
                        x as f64 * lumen_core::events::WHEEL_LINE_PX,
                        -(y as f64) * lumen_core::events::WHEEL_LINE_PX,
                    ),
                    MouseScrollDelta::PixelDelta(p) => Vec2::new(p.x, -p.y),
                };
                self.inject(Event::Wheel(WheelEvent {
                    pos: self.cursor,
                    delta: d,
                    modifiers: self.modifiers,
                }));
            }
            // P.3e: OS drag-and-drop. winit handles the platform protocol
            // (XDND here) and delivers one DroppedFile per file; each becomes
            // a portable Drop through the one input queue — the same event
            // headless tests and the agent's input.drop synthesize. Position
            // is the last-known cursor (X11 does not report a drop point).
            WindowEvent::DroppedFile(path) => {
                let pos = self.cursor;
                self.inject(Event::Drop(DropEvent {
                    pos,
                    data: DropData {
                        text: None,
                        files: vec![path.display().to_string()],
                    },
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
                if event.state == ElementState::Pressed
                    && !self.ime_active
                    && !is_command_chord(self.modifiers)
                {
                    if let Some(t) = &event.text {
                        if !t.is_empty() && !t.chars().all(char::is_control) {
                            self.inject(Event::TextInput(TextInputEvent {
                                text: t.to_string(),
                            }));
                        }
                    }
                }
                if let Some(k) = map_key(&event.logical_key) {
                    // P.3a: a Ctrl-chord may paste — pull the OS clipboard
                    // into the portable Runtime clipboard first, so Ctrl+V
                    // commits what the desktop actually holds.
                    if self.modifiers.contains(Modifiers::CTRL)
                        && event.state == ElementState::Pressed
                    {
                        if let Some(cb) = &mut self.os_clipboard {
                            if let Ok(text) = cb.get_text() {
                                if text != self.os_clip_last {
                                    self.os_clip_last = text.clone();
                                    if let Some(h) = &mut self.headless {
                                        h.clipboard_write(text);
                                    }
                                }
                            }
                        }
                    }
                    // P.3c: menu accelerators win over widget key handling —
                    // exactly like a native menubar chord. On Linux this is
                    // the only native activation path (no menubar under
                    // winit); on Windows/macOS muda usually consumes the
                    // chord first, so this is the portable fallback.
                    if event.state == ElementState::Pressed && !event.repeat && !self.ime_active {
                        let target = self
                            .headless
                            .as_ref()
                            .and_then(|h| accel_target(h.menu(), self.modifiers, &k));
                        if let Some(id) = target {
                            if let Some(h) = &mut self.headless {
                                h.activate_menu(&id);
                            }
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                    }
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
                let mut pending_tray: Option<(Vec<String>, lumen_widgets::system::MenuModel)> =
                    None;
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
                        #[cfg(feature = "wgpu")]
                        if self.direct {
                            h.resize_surface(pw, ph);
                        }
                        if let Some(p) = &mut self.presenter {
                            p.resize(pw, ph);
                        }
                        h.prepare_resize(self.size, self.scale);
                    }
                    // Present only when the frame actually changed (R2): an idle
                    // tick repaints nothing, so the surface keeps its last frame.
                    let stats = h.pump();
                    // PROP1: apply the `cursor` for whatever the pointer is
                    // over. Only on CHANGE — `set_cursor` is a platform call per
                    // invocation, and a frame does not need to re-assert a shape
                    // that is already showing.
                    let want = h.cursor_shape();
                    if want != self.cursor_shape {
                        self.cursor_shape = want;
                        if let Some(w) = &self.window {
                            match want {
                                // No rule applies ⇒ the platform default. This
                                // arm used to leave whatever was showing, on the
                                // theory that a drag or an IME might have set
                                // it — but nothing else in the stack sets the
                                // cursor, and the practical effect was that the
                                // first hand or I-beam the pointer touched stuck
                                // to it everywhere until it happened to cross
                                // another node with a rule. The hovered node is
                                // the authority, and "no rule" is `default`,
                                // exactly as it is in CSS.
                                None => {
                                    w.set_cursor_visible(true);
                                    w.set_cursor(winit_cursor(lumen_core::CursorShape::Default));
                                }
                                Some(lumen_core::CursorShape::None) => w.set_cursor_visible(false),
                                Some(shape) => {
                                    w.set_cursor_visible(true);
                                    w.set_cursor(winit_cursor(shape));
                                }
                            }
                        }
                    }
                    // P.3b: fulfil recorded system requests natively (file
                    // dialogs are modal; the loop resumes after the pick).
                    let mut reqs = h.take_system_requests();
                    // P.3e/M.6: tray + exit requests are shell state, not
                    // fulfilment — split them out (tray applied after this
                    // block for borrow reasons; exit ends the loop).
                    let mut tray_tips = Vec::new();
                    let mut exit = false;
                    reqs.retain(|r| match r {
                        lumen_widgets::system::SystemRequest::TrayTooltip(t) => {
                            tray_tips.push(t.clone());
                            false
                        }
                        lumen_widgets::system::SystemRequest::Exit => {
                            exit = true;
                            false
                        }
                        _ => true,
                    });
                    if exit {
                        el.exit();
                        return;
                    }
                    if !reqs.is_empty() {
                        fulfill_system_requests(h, reqs, native_dialog_resolver);
                    }
                    if !tray_tips.is_empty() {
                        pending_tray = Some((tray_tips, h.menu().clone()));
                    }
                    // P.3c: realize a newly installed menu model natively.
                    // (Attach is a no-op on Linux — see `attach_native_menu`.)
                    #[cfg(feature = "desktop-integration")]
                    if h.menu_rev() != self.menu_rev_seen {
                        self.menu_rev_seen = h.menu_rev();
                        let menu = build_native_menu(h.menu());
                        if let Some(w) = &self.window {
                            attach_native_menu(&menu, w);
                        }
                        self.native_menu = Some(menu);
                    }
                    // P.3a: a copy inside the app updated the portable
                    // clipboard — mirror it out to the OS.
                    let app_clip = h.clipboard_read();
                    if !app_clip.is_empty() && app_clip != self.os_clip_last {
                        if let Some(cb) = &mut self.os_clipboard {
                            if cb.set_text(app_clip.clone()).is_ok() {
                                self.os_clip_last = app_clip;
                            }
                        }
                    }
                    let force = std::mem::take(&mut self.force_present);
                    if stats.painted || resized || force {
                        #[cfg(feature = "wgpu")]
                        if self.direct {
                            // GPU → swapchain directly, no readback (1c).
                            match h.present_to_surface() {
                                Present::Done => {}
                                // The swapchain went stale mid-resize (or the
                                // acquire timed out). Routine during a drag and
                                // routine to recover from: ask for one more
                                // redraw and present the same list again.
                                //
                                // This arm is the bug fix. It used to share the
                                // fallback path below, so ONE dropped frame
                                // during a resize built a second wgpu surface on
                                // a window that was still being dragged — and a
                                // configure that races a resize is a FATAL
                                // `Invalid surface` panic in wgpu 22, not an
                                // error we could catch.
                                Present::Skipped => {
                                    self.force_present = true;
                                    // Routine during a resize drag, a stopped
                                    // window in bulk. Throttled so the drag
                                    // stays quiet and a sustained run does not.
                                    self.skipped_presents += 1;
                                    if self.skipped_presents.is_multiple_of(60) {
                                        h.runtime().log(
                                            "warn",
                                            format!(
                                                "{} presents skipped since start — \
                                                 routine during a resize drag, but \
                                                 a sustained run means the window \
                                                 has stopped updating",
                                                self.skipped_presents
                                            ),
                                        );
                                    }
                                    if let Some(w) = &self.window {
                                        w.request_redraw();
                                    }
                                }
                                // The surface is gone for good — the window
                                // outgrew the device's texture limit, where
                                // configuring is fatal rather than recoverable.
                                // Degrade to CPU presentation for the rest of
                                // the session instead of freezing: the window
                                // keeps updating, just through a readback.
                                Present::Unavailable => {
                                    self.direct = false;
                                    self.presenter = self.window.clone().and_then(Presenter::new);
                                    // A PERMANENT per-frame readback for the
                                    // rest of the session — the single largest
                                    // silent perf change the shell can make.
                                    h.runtime().log(
                                        "warn",
                                        "present degraded to cpu-readback for the \
                                         rest of this session: no usable surface \
                                         for this device. Every frame now pays a \
                                         GPU→CPU readback.",
                                    );
                                    eprintln!(
                                        "lumen: present = cpu-readback (no usable \
                                         surface for this device; falling back)"
                                    );
                                }
                            }
                        }
                        // `presenter` is None in direct mode, so this covers
                        // both paths without a second `direct` test.
                        if let Some(p) = &mut self.presenter {
                            let frame = h.screenshot();
                            p.present(&frame);
                        }
                        // P.4: publish the new semantic tree to any
                        // subscribed AT (no-op — the closure never runs —
                        // when none is active).
                        #[cfg(feature = "accessibility")]
                        if let Some(a) = &mut self.a11y {
                            a.update_if_active(|| {
                                lumen_widgets::a11y::build_tree(&h.semantics_elided())
                            });
                        }
                    }
                }
                #[cfg(feature = "desktop-integration")]
                if let Some((tips, model)) = pending_tray {
                    for t in tips {
                        self.tray_update(&t, &model);
                    }
                }
                // GX2: without the tray backend the requests are drained and
                // dropped rather than queueing forever.
                #[cfg(not(feature = "desktop-integration"))]
                let _ = pending_tray;
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
        // P.3d-2: the loop sleeps until the EARLIEST deadline across every
        // window (each pipeline has its own virtual clock); a window already
        // due gets a redraw and the loop polls.
        let mut poll = false;
        let mut min_dt: Option<f64> = None;
        if let Some(h) = &self.headless {
            match h.next_deadline() {
                None => {}
                Some(t) if t <= h.now_ms() => {
                    poll = true;
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                Some(t) => min_dt = Some(t - h.now_ms()),
            }
        }
        for sw in self.secondary.values() {
            match sw.headless.next_deadline() {
                None => {}
                Some(t) if t <= sw.headless.now_ms() => {
                    poll = true;
                    sw.window.request_redraw();
                }
                Some(t) => {
                    let dt = t - sw.headless.now_ms();
                    min_dt = Some(min_dt.map_or(dt, |m: f64| m.min(dt)));
                }
            }
        }
        if poll {
            el.set_control_flow(ControlFlow::Poll);
        } else if let Some(dt) = min_dt {
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_secs_f64(dt.max(0.0) / 1000.0),
            ));
        } else {
            el.set_control_flow(ControlFlow::Wait);
        }
    }
}

impl<E: lumen_core::tasks::Spawner, P: lumen_widgets::app::PlatformConfig> Shell<E, P> {
    /// P.3d-2: schedule a frame for every window — an input or state change
    /// anywhere may re-render any window (shared signals); untouched windows
    /// pump as dirty-checked no-ops.
    fn redraw_all(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        for sw in self.secondary.values() {
            sw.window.request_redraw();
        }
    }

    /// P.3d-2: realize one declared secondary window (its own winit window,
    /// renderer, and `Headless` pipeline over the shared `Runtime`).
    fn open_secondary(&mut self, el: &ActiveEventLoop, d: &lumen_widgets::system::WindowDesc) {
        let Some(main) = &self.headless else { return };
        #[cfg(feature = "wgpu")]
        let renderer: ShellRenderer = lumen_widgets::renderer_override()
            .unwrap_or_else(|| Box::new(lumen_render::WgpuFallbackTinySkia::new()));
        #[cfg(not(feature = "wgpu"))]
        let renderer: ShellRenderer =
            lumen_widgets::renderer_override().unwrap_or_else(|| Box::new(lumen_render::TinySkia));
        let Some(mut h) = main.open_window_with(
            &d.id,
            renderer,
            lumen_core::tasks::ThreadPoolSpawner::new(1),
        ) else {
            eprintln!("lumen: window '{}' has no declaration", d.id);
            main.runtime().log(
                "warn",
                format!("window `{}` was requested but has no declaration", d.id),
            );
            return;
        };
        let attrs = Window::default_attributes()
            .with_title(&d.title)
            .with_inner_size(winit::dpi::LogicalSize::new(d.width, d.height));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("lumen: window '{}': {e}", d.id);
                main.runtime()
                    .log("warn", format!("window `{}` failed to open: {e}", d.id));
                return;
            }
        };
        let scale = window.scale_factor();
        let phys = window.inner_size();
        let size = Size::new(
            (phys.width.max(1) as f64 / scale).max(1.0),
            (phys.height.max(1) as f64 / scale).max(1.0),
        );
        h.prepare_resize(size, scale);
        #[cfg(feature = "wgpu")]
        let direct = h.attach_surface(window.clone().into(), phys.width.max(1), phys.height.max(1));
        #[cfg(not(feature = "wgpu"))]
        let direct = false;
        let presenter = if direct {
            None
        } else {
            Presenter::new(window.clone())
        };
        let proxy = self.proxy.clone();
        h.set_waker(std::sync::Arc::new(move || {
            let _ = proxy.send_event(ShellEvent::Wake);
        }));
        window.request_redraw();
        self.secondary.insert(
            window.id(),
            SecondaryWindow {
                headless: h,
                window,
                presenter,
                direct,
                size,
                scale,
                cursor: Point::ZERO,
                last_frame: Instant::now(),
                pending_resize: false,
            },
        );
    }

    /// P.3d-2: the secondary-window event path.
    fn secondary_event(&mut self, _el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            self.secondary.remove(&id); // dropping the window closes it
            return;
        }
        // Inputs that can change shared state fan a redraw out to all windows.
        let fan_out = matches!(
            event,
            WindowEvent::MouseInput { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::DroppedFile(_)
        );
        let modifiers = self.modifiers;
        let Some(sw) = self.secondary.get_mut(&id) else {
            return;
        };
        match event {
            WindowEvent::Resized(s) => {
                sw.scale = sw.window.scale_factor();
                sw.size = Size::new(
                    (s.width.max(1) as f64 / sw.scale).max(1.0),
                    (s.height.max(1) as f64 / sw.scale).max(1.0),
                );
                sw.pending_resize = true;
                sw.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                sw.scale = scale_factor;
                sw.pending_resize = true;
                sw.window.request_redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = map_modifiers(m.state());
            }
            WindowEvent::CursorMoved { position, .. } => {
                sw.cursor = Point::new(position.x / sw.scale, position.y / sw.scale);
                let mut ev = PointerEvent::at(sw.cursor);
                ev.modifiers = modifiers;
                sw.headless.inject(Event::PointerMove(ev));
                sw.window.request_redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mut ev = PointerEvent::at(sw.cursor);
                ev.button = map_button(button);
                ev.modifiers = modifiers;
                sw.headless.inject(if state == ElementState::Pressed {
                    Event::PointerDown(ev)
                } else {
                    Event::PointerUp(ev)
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    // Same sign convention as the primary window: positive dy
                    // scrolls toward the end. This negation was MISSING here, so
                    // every secondary window scrolled inverted — and no test
                    // caught it, because the multi-window tests inject events
                    // directly and never exercise this translation.
                    MouseScrollDelta::LineDelta(x, y) => (
                        f64::from(x) * lumen_core::events::WHEEL_LINE_PX,
                        -f64::from(y) * lumen_core::events::WHEEL_LINE_PX,
                    ),
                    MouseScrollDelta::PixelDelta(p) => (p.x, -p.y),
                };
                sw.headless.inject(Event::Wheel(WheelEvent {
                    pos: sw.cursor,
                    delta: Vec2::new(dx, dy),
                    modifiers,
                }));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Direct text + keys; IME composition stays main-window (v1).
                if event.state == ElementState::Pressed && !is_command_chord(modifiers) {
                    if let Some(t) = &event.text {
                        if !t.is_empty() && !t.chars().all(char::is_control) {
                            sw.headless.inject(Event::TextInput(TextInputEvent {
                                text: t.to_string(),
                            }));
                        }
                    }
                }
                if let Some(k) = map_key(&event.logical_key) {
                    let ke = KeyEvent {
                        key: k,
                        modifiers,
                        repeat: event.repeat,
                    };
                    sw.headless.inject(if event.state == ElementState::Pressed {
                        Event::KeyDown(ke)
                    } else {
                        Event::KeyUp(ke)
                    });
                }
            }
            WindowEvent::DroppedFile(path) => {
                let pos = sw.cursor;
                sw.headless.inject(Event::Drop(DropEvent {
                    pos,
                    data: DropData {
                        text: None,
                        files: vec![path.display().to_string()],
                    },
                }));
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed_ms = (now - sw.last_frame).as_secs_f64() * 1000.0;
                sw.last_frame = now;
                sw.headless.advance_clock(elapsed_ms.min(1000.0));
                if std::mem::take(&mut sw.pending_resize) {
                    let pw = (sw.size.width * sw.scale).round().max(1.0) as u32;
                    let ph = (sw.size.height * sw.scale).round().max(1.0) as u32;
                    #[cfg(feature = "wgpu")]
                    if sw.direct {
                        sw.headless.resize_surface(pw, ph);
                    }
                    if let Some(p) = &mut sw.presenter {
                        p.resize(pw, ph);
                    }
                    sw.headless.prepare_resize(sw.size, sw.scale);
                }
                let stats = sw.headless.pump();
                if stats.painted {
                    #[cfg(feature = "wgpu")]
                    if sw.direct {
                        match sw.headless.present_to_surface() {
                            Present::Done => {}
                            // Same recovery as the primary window: a stale
                            // swapchain during a resize costs one frame, not the
                            // surface.
                            Present::Skipped => sw.window.request_redraw(),
                            Present::Unavailable => {
                                sw.direct = false;
                                sw.presenter = Presenter::new(sw.window.clone());
                            }
                        }
                    }
                    if let Some(p) = &mut sw.presenter {
                        let frame = sw.headless.screenshot();
                        p.present(&frame);
                    }
                }
            }
            _ => {}
        }
        if fan_out {
            self.redraw_all();
        }
    }

    /// P.4: publish the current semantic tree to the AT (used for the
    /// initial-tree request; per-frame updates ride `RedrawRequested`).
    #[cfg(feature = "accessibility")]
    fn push_a11y_tree(&mut self) {
        let Some(h) = &self.headless else { return };
        if let Some(a) = &mut self.a11y {
            a.update_if_active(|| lumen_widgets::a11y::build_tree(&h.semantics_elided()));
        }
    }

    fn inject(&mut self, ev: Event) {
        if let Some(h) = &mut self.headless {
            h.inject(ev);
        }
        // Event-driven: redraw only after input — every window, since the
        // shared store may re-render any of them (P.3d-2).
        self.redraw_all();
    }
}

// --- P.3e: system tray --------------------------------------------------------

#[cfg(feature = "desktop-integration")]
/// The realized tray. Linux: a channel into the tray's gtk thread; other
/// platforms hold the icon on the loop thread.
enum TrayState {
    #[cfg(target_os = "linux")]
    Channel(mpsc::Sender<String>),
    #[cfg(not(target_os = "linux"))]
    Direct(tray_icon::TrayIcon),
}

/// A 16×16 solid-accent icon generated in code — identifies the tray slot
/// without shipping an asset (apps get real icon support with `lumen
/// package` branding, E.1).
#[cfg(feature = "desktop-integration")]
fn tray_icon_pixels() -> tray_icon::Icon {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        rgba.extend_from_slice(&[0x4a, 0x7c, 0xff, 0xff]);
    }
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("tray icon")
}

/// Linux: run the tray on its own gtk thread (appindicator requires a gtk
/// loop; winit owns the main one). Appindicator has no tooltips, so the text
/// lands as the *title* (shown beside the icon) as well. The tray's context
/// menu hosts the app's `MenuModel` — the menu Linux can't show as a winit
/// menubar gets a native home here, and **ayatana appindicator silently
/// refuses to register without a menu** (found live: no menu ⇒ no
/// StatusNotifierItem, no error). Item clicks arrive on muda's event
/// handler → `ShellEvent::Menu` → `activate_menu`.
#[cfg(target_os = "linux")]
#[cfg(feature = "desktop-integration")]
fn spawn_tray(initial: String, model: lumen_widgets::system::MenuModel) -> TrayState {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        if gtk::init().is_err() {
            eprintln!("lumen tray: gtk init failed");
            return;
        }
        let menu = if model.items.is_empty() {
            // Registration requires *some* menu; a lone disabled entry keeps
            // the icon alive for apps that only set a tooltip.
            let m = muda::Menu::new();
            let _ = m.append(&muda::MenuItem::with_id("tray", "Lumen", false, None));
            m
        } else {
            build_native_menu(&model)
        };
        let tray = match tray_icon::TrayIconBuilder::new()
            .with_icon(tray_icon_pixels())
            .with_title(&initial)
            .with_tooltip(&initial)
            .with_menu(Box::new(menu))
            .build()
        {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lumen tray: {e}");
                return;
            }
        };
        // A real glib main loop is required: appindicator's watcher
        // handshake (RegisterStatusNotifierItem + property callbacks)
        // dispatches on this thread's default main context — a hand-rolled
        // `main_iteration_do` poll starves it and the item never registers.
        gtk::glib::timeout_add_local(Duration::from_millis(200), move || {
            while let Ok(text) = rx.try_recv() {
                tray.set_title(Some(&text));
                let _ = tray.set_tooltip(Some(&text));
            }
            gtk::glib::ControlFlow::Continue
        });
        gtk::main();
    });
    TrayState::Channel(tx)
}

#[cfg(not(target_os = "linux"))]
#[cfg(feature = "desktop-integration")]
fn spawn_tray(initial: String, model: lumen_widgets::system::MenuModel) -> TrayState {
    let mut builder = tray_icon::TrayIconBuilder::new()
        .with_icon(tray_icon_pixels())
        .with_tooltip(&initial);
    if !model.items.is_empty() {
        builder = builder.with_menu(Box::new(build_native_menu(&model)));
    }
    match builder.build() {
        Ok(t) => TrayState::Direct(t),
        Err(e) => {
            eprintln!("lumen tray: {e}");
            // Keep a dead channel-less placeholder impossible: retry next time.
            panic!("tray creation failed: {e}");
        }
    }
}

impl<E: lumen_core::tasks::Spawner, P: lumen_widgets::app::PlatformConfig> Shell<E, P> {
    /// Apply a `TrayTooltip` request: create the tray on first use, then
    /// push the text.
    #[cfg(feature = "desktop-integration")]
    fn tray_update(&mut self, text: &str, model: &lumen_widgets::system::MenuModel) {
        match &mut self.tray {
            None => self.tray = Some(spawn_tray(text.to_string(), model.clone())),
            #[cfg(target_os = "linux")]
            Some(TrayState::Channel(tx)) => {
                let _ = tx.send(text.to_string());
            }
            #[cfg(not(target_os = "linux"))]
            Some(TrayState::Direct(t)) => {
                let _ = t.set_tooltip(Some(text));
            }
        }
    }
}

// --- P.4: assistive-technology action routing --------------------------------

/// PROP1: map the first-party [`CursorShape`](lumen_core::CursorShape) onto
/// winit's icon set. The enum is first-party so the style engine and runtime
/// never name winit; this is the single place the two vocabularies meet.
fn winit_cursor(shape: lumen_core::CursorShape) -> winit::window::CursorIcon {
    use lumen_core::CursorShape as C;
    use winit::window::CursorIcon as W;
    match shape {
        C::Default | C::None => W::Default,
        C::Pointer => W::Pointer,
        C::Text => W::Text,
        C::Wait => W::Wait,
        C::Crosshair => W::Crosshair,
        C::Move => W::Move,
        C::ColResize => W::ColResize,
        C::RowResize => W::RowResize,
        C::NotAllowed => W::NotAllowed,
    }
}

/// Whether to construct the AccessKit adapter for a newly created window.
///
/// # Why this is a switch and not a deferral (GX4)
///
/// Constructing the adapter is neither free nor dormant. On Linux,
/// `accesskit_unix::Adapter::new` calls `get_or_init_messages`, which
/// **unconditionally spawns a thread** that opens a D-Bus session connection and
/// runs an event loop — whether or not any assistive technology is present.
/// Only *tree publication* waits for an AT to activate. `03-spec-semantics-agent.md`
/// claimed the adapter itself was "dormant until an AT subscribes"; that was
/// true of the tree, not of the thread, and the spec has been corrected.
///
/// It cannot be made lazy from this side: detecting that an AT has appeared
/// requires the very D-Bus connection the adapter owns, so "create it when
/// something asks" is circular. The honest options are on or off, which is why
/// this is an opt-out rather than the deferral the plan originally named.
///
/// **Defaults to on.** Accessibility is a correctness property, not an
/// optimization, so a constrained profile opts out explicitly rather than
/// having to opt in. `NO_AT_BRIDGE=1` is honoured because GTK and Qt already
/// use it, so a user who has disabled the AT bridge system-wide should not have
/// to learn a Lumen-specific variable to be obeyed.
#[cfg(feature = "accessibility")]
fn a11y_enabled() -> bool {
    a11y_enabled_from(
        std::env::var("LUMEN_A11Y").ok().as_deref(),
        std::env::var("NO_AT_BRIDGE").ok().as_deref(),
    )
}

/// The decision in [`a11y_enabled`], as a pure function of the two variables so
/// it can be tested without mutating process-global environment state.
#[cfg(feature = "accessibility")]
fn a11y_enabled_from(lumen_a11y: Option<&str>, no_at_bridge: Option<&str>) -> bool {
    if let Some(v) = lumen_a11y {
        // An explicit Lumen setting wins over the ecosystem variable, in both
        // directions — including re-enabling under `NO_AT_BRIDGE=1`.
        return !matches!(v.trim(), "0" | "off" | "false" | "no");
    }
    !matches!(no_at_bridge.map(str::trim), Some("1") | Some("true"))
}

/// Route an AT [`accesskit::ActionRequest`] into the one input queue. The
/// target id is the runtime node index (the same ids `build_tree` publishes);
/// `Click` synthesizes the standard down/up pair at the node's center — the
/// exact shape the agent's `input.click` and the live pointer produce.
/// (Focus/scroll actions are documented-unrouted until a headless focus-by-
/// node API exists; Tab-order focus already works through key events.)
#[cfg(feature = "accessibility")]
fn route_at_action<
    R: lumen_render::Renderer,
    E: lumen_core::tasks::Spawner,
    P: lumen_widgets::app::PlatformConfig,
>(
    h: &mut Headless<R, E, P>,
    req: &accesskit::ActionRequest,
) {
    let root = h.semantics_elided();
    // ID1: published ids are `NodeHandle::fold64()` (see
    // `lumen_widgets::a11y::build_tree`), so compare the same projection. The
    // old `& 0xFFFF_FFFF` masked out an arena index, which stops identifying
    // anything once the arena recycles slots.
    // accesskit 0.24 split `target` into `target_tree` + `target_node` for its
    // new subtree support. Lumen publishes only `TreeId::ROOT` (see
    // `a11y::build_tree`), so the node half is the whole address here.
    //
    // A11Y2c: the decision is `a11y::resolve_at_action`, which is pure and
    // therefore unit-tested (`lumen-widgets/tests/at_actions.rs`). This
    // function is left with the part that genuinely needs a live app —
    // injecting and pumping.
    let Some(cmd) = lumen_widgets::a11y::resolve_at_action(
        &root,
        req.target_node.0,
        req.action,
        req.data.as_ref(),
    ) else {
        return;
    };
    match cmd {
        lumen_widgets::a11y::AtCommand::Click(p) => {
            h.inject(Event::PointerDown(PointerEvent::at(p)));
            h.inject(Event::PointerUp(PointerEvent::at(p)));
        }
        lumen_widgets::a11y::AtCommand::Wheel { pos, delta } => {
            h.inject(Event::Wheel(lumen_core::events::WheelEvent {
                pos,
                delta,
                modifiers: lumen_core::events::Modifiers::empty(),
            }));
        }
    }
    h.pump();
}

// --- P.3c: native menus (muda) + portable accelerators ----------------------

/// Parse a portable accelerator chord — `"Ctrl+O"`, `"Ctrl+Shift+S"`,
/// `"Alt+Enter"` — into (modifiers, key). Modifier tokens: `Ctrl`/`Control`,
/// `Shift`, `Alt`/`Option`, `Meta`/`Cmd`/`Super` (and `CmdOrCtrl`, which maps
/// to Meta on macOS and Ctrl elsewhere). The final token is the key: a single
/// character (matched case-insensitively) or a named key.
fn parse_accel(chord: &str) -> Option<(Modifiers, Key)> {
    let mut mods = Modifiers::empty();
    let mut key = None;
    for part in chord.split('+') {
        let p = part.trim();
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CTRL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" | "option" => mods |= Modifiers::ALT,
            "meta" | "cmd" | "super" | "command" => mods |= Modifiers::META,
            "cmdorctrl" => {
                mods |= if cfg!(target_os = "macos") {
                    Modifiers::META
                } else {
                    Modifiers::CTRL
                }
            }
            "enter" | "return" => key = Some(Key::Named(NamedKey::Enter)),
            "escape" | "esc" => key = Some(Key::Named(NamedKey::Escape)),
            "tab" => key = Some(Key::Named(NamedKey::Tab)),
            "space" => key = Some(Key::Named(NamedKey::Space)),
            "backspace" => key = Some(Key::Named(NamedKey::Backspace)),
            "delete" | "del" => key = Some(Key::Named(NamedKey::Delete)),
            "left" => key = Some(Key::Named(NamedKey::ArrowLeft)),
            "right" => key = Some(Key::Named(NamedKey::ArrowRight)),
            "up" => key = Some(Key::Named(NamedKey::ArrowUp)),
            "down" => key = Some(Key::Named(NamedKey::ArrowDown)),
            other if other.chars().count() == 1 => {
                key = Some(Key::Character(other.into()));
            }
            _ => return None,
        }
    }
    key.map(|k| (mods, k))
}

/// Does the pressed (modifiers, key) match `chord`? Character keys compare
/// case-insensitively (Shift is part of the chord, not the character).
fn accel_matches(chord: &str, mods: Modifiers, key: &Key) -> bool {
    let Some((am, ak)) = parse_accel(chord) else {
        return false;
    };
    if am != mods {
        return false;
    }
    match (&ak, key) {
        (Key::Named(a), Key::Named(b)) => a == b,
        (Key::Character(a), Key::Character(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// Find the enabled menu item whose accelerator matches the pressed chord
/// (depth-first; disabled subtrees are skipped like a native menu would).
fn accel_target(
    model: &lumen_widgets::system::MenuModel,
    mods: Modifiers,
    key: &Key,
) -> Option<String> {
    fn walk(
        items: &[lumen_widgets::system::MenuItem],
        mods: Modifiers,
        key: &Key,
    ) -> Option<String> {
        items.iter().filter(|i| i.enabled).find_map(|i| {
            i.accel
                .as_deref()
                .filter(|a| accel_matches(a, mods, key))
                .map(|_| i.id.clone())
                .or_else(|| walk(&i.children, mods, key))
        })
    }
    walk(&model.items, mods, key)
}

/// Realize the portable [`MenuModel`](lumen_widgets::system::MenuModel) as a
/// muda menu (same ids, so a native click reports the portable command id).
#[cfg(feature = "desktop-integration")]
fn build_native_menu(model: &lumen_widgets::system::MenuModel) -> muda::Menu {
    fn native(item: &lumen_widgets::system::MenuItem) -> Box<dyn muda::IsMenuItem> {
        if item.children.is_empty() {
            // Native accelerator registration is best-effort: muda's parser
            // covers the same `Mod+Key` grammar; an unparsable chord still
            // works through the shell's own matcher.
            let accel = item
                .accel
                .as_deref()
                .and_then(|a| a.parse::<muda::accelerator::Accelerator>().ok());
            Box::new(muda::MenuItem::with_id(
                item.id.clone(),
                &item.label,
                item.enabled,
                accel,
            ))
        } else {
            let sub = muda::Submenu::with_id(item.id.clone(), &item.label, item.enabled);
            for c in &item.children {
                let ci = native(c);
                let _ = sub.append(ci.as_ref());
            }
            Box::new(sub)
        }
    }
    let menu = muda::Menu::new();
    for item in &model.items {
        let ni = native(item);
        let _ = menu.append(ni.as_ref());
    }
    menu
}

/// Attach the realized menu as the window's native menubar.
#[cfg(target_os = "windows")]
#[cfg(feature = "desktop-integration")]
fn attach_native_menu(menu: &muda::Menu, window: &Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(h) = window.window_handle() {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            unsafe {
                let _ = menu.init_for_hwnd(w.hwnd.get());
            }
        }
    }
}

/// Attach the realized menu as the application menu (macOS menubar).
#[cfg(target_os = "macos")]
#[cfg(feature = "desktop-integration")]
fn attach_native_menu(menu: &muda::Menu, _window: &Window) {
    menu.init_for_nsapp();
}

/// Linux: muda's backend is GTK — a winit X11/Wayland window has no menubar
/// attachment point (the limitation is winit-wide, not Lumen's). The menu
/// stays observable data; activation paths here are accelerator chords
/// (matched by the shell's key handler) and the agent's `menu.invoke`.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[cfg(feature = "desktop-integration")]
fn attach_native_menu(_menu: &muda::Menu, _window: &Window) {}

/// Whether these modifiers make the keypress a *command*, not typing.
///
/// A chord like Ctrl+A is a command; the platform may still resolve it to the
/// character "a" (X11 and macOS both do), and committing that would type an "a"
/// into the field before the command ran — which is exactly what Ctrl+A used to
/// do. Ctrl+Alt is excluded: Windows maps AltGr to it, and AltGr genuinely
/// produces characters on most European layouts.
fn is_command_chord(m: Modifiers) -> bool {
    let cmd = m.contains(Modifiers::CTRL) || m.contains(Modifiers::META);
    cmd && !m.contains(Modifiers::ALT)
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

/// Presents a CPU-rendered frame with **softbuffer** — no GPU (ADR-003
/// amendment, 2026-08-08).
///
/// This is what makes a no-GPU desktop build possible at all. Before it, the
/// shell had exactly one presentation path and it went through wgpu, so
/// `01 §9`'s `<5 MB` budget and CFG1's "no-GPU, software-render" profile were
/// unreachable by any combination of feature flags — the capability was absent,
/// not merely unconfigured.
///
/// Deliberately the same three-method shape as the wgpu presenter
/// (`new`/`resize`/`present`), so the call sites do not branch: which one exists
/// is a compile-time choice, not a runtime one.
#[cfg(not(feature = "wgpu"))]
struct Presenter {
    /// `Context` must outlive every `Surface` made from it, so it is held here
    /// rather than dropped after construction.
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    size: (u32, u32),
}

#[cfg(not(feature = "wgpu"))]
impl Presenter {
    /// Fallible for the same reason as the wgpu presenter: this also runs
    /// mid-session on the fallback path, where a panic kills a running app.
    fn new(window: Arc<Window>) -> Option<Presenter> {
        let context = softbuffer::Context::new(window.clone()).ok()?;
        let surface = softbuffer::Surface::new(&context, window.clone()).ok()?;
        let phys = window.inner_size();
        let mut p = Presenter {
            _context: context,
            surface,
            size: (0, 0),
        };
        p.resize(phys.width.max(1), phys.height.max(1));
        Some(p)
    }

    fn resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if self.size == (w, h) {
            return;
        }
        if let (Some(nw), Some(nh)) = (std::num::NonZeroU32::new(w), std::num::NonZeroU32::new(h)) {
            if self.surface.resize(nw, nh).is_ok() {
                self.size = (w, h);
            }
        }
    }

    fn present(&mut self, frame: &RgbaImage) {
        let (fw, fh) = (frame.width().max(1), frame.height().max(1));
        self.resize(fw, fh);
        let Ok(mut buf) = self.surface.buffer_mut() else {
            return;
        };
        // softbuffer wants 0RGB in a u32 per pixel, host-endian; the frame is
        // straight (non-premultiplied) RGBA8. The window is opaque, so alpha is
        // dropped rather than composited — matching the wgpu presenter, which
        // clears to opaque white and blits over it.
        let px = frame.pixels();
        let n = (fw as usize * fh as usize).min(buf.len());
        for i in 0..n {
            let p = &px[i * 4..i * 4 + 4];
            buf[i] = ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | (p[2] as u32);
        }
        let _ = buf.present();
    }
}

/// Presents a CPU-rendered frame to a wgpu surface via a fullscreen blit.
#[cfg(feature = "wgpu")]
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

#[cfg(feature = "wgpu")]
impl Presenter {
    /// Build a CPU-frame presenter for `window`, or `None` if this machine
    /// cannot give us one.
    ///
    /// Fallible on purpose. This runs at startup on the no-GPU path, where a
    /// panic is at least honest — but it ALSO runs mid-session when the direct
    /// path gives up, and killing a running app because an adapter request
    /// failed at the worst moment is not a trade anyone chose.
    fn new(window: Arc<Window>) -> Option<Presenter> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).ok()?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .ok()?;
        // MemoryUsage over the default Performance hint — the blit presenter holds
        // one small texture; no need for large pre-reserved GPU pools.
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .ok()?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        // Sampled HERE, not before the adapter/device requests: those block for
        // milliseconds, and this constructor now runs mid-resize on the fallback
        // path. Configuring with a size the window has already left behind is
        // exactly the race that panics — `Surface::configure` reports a stale
        // window as `InvalidSurface` and wgpu 22 routes that through
        // `handle_error_fatal`, so it aborts the process instead of returning an
        // error. Re-querying does not close the race (nothing can, from out
        // here), but it removes the part of it we were creating ourselves.
        let size = window.inner_size();
        // The same ceiling `Wgpu` applies: configuring past the device's
        // `max_texture_dimension_2d` is fatal too, so clamp rather than trip it.
        let max = adapter.limits().max_texture_dimension_2d.max(1);
        let config = wgpu::SurfaceConfiguration {
            color_space: wgpu::SurfaceColorSpace::Srgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.clamp(1, max),
            height: size.height.clamp(1, max),
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
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Some(Presenter {
            surface,
            device,
            queue,
            config,
            pipeline,
            bgl,
            sampler,
            staging: None,
        })
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
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.pixels(),
            wgpu::TexelCopyBufferLayout {
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
        use wgpu::CurrentSurfaceTexture as Acquired;
        let surface_tex = match self.surface.get_current_texture() {
            Acquired::Success(t) | Acquired::Suboptimal(t) => t,
            Acquired::Outdated => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Acquired::Success(t) | Acquired::Suboptimal(t) => t,
                    _ => return,
                }
            }
            // Nothing to present onto right now; the next frame tries again.
            // This path has no fallback of its own — it IS the fallback.
            _ => return,
        };
        let sview = surface_tex.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &sview,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
        // 30: presenting is a queue operation.
        self.queue.present(surface_tex);
    }
}

#[cfg(feature = "wgpu")]
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

#[cfg(feature = "wgpu")]
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

    // --- C.5: the agent endpoint's two security guards -------------------
    //
    // These had no test at all. `lumen-shell` has no tests/ directory, and
    // the only LUMEN_AGENT_TOKEN test in the repo is in lumen-cli and asserts
    // the CLIENT attaches the token — nothing asserted the server enforces
    // it, or that it refuses to go public without one.

    #[test]
    fn loopback_binds_are_recognised() {
        for addr in [
            "127.0.0.1:9230",
            "127.1.2.3:80",
            "[::1]:9230",
            "localhost:9230",
            "LocalHost:9230",
        ] {
            assert!(is_loopback_addr(addr), "{addr} should count as loopback");
        }
    }

    #[test]
    fn public_binds_are_not_loopback() {
        for addr in [
            "0.0.0.0:9230",
            "192.168.1.5:9230",
            "10.0.0.1:9230",
            "[::]:9230",
            "example.com:9230",
            "",
            // Narrowed vs the old string check, deliberately. Both fail CLOSED
            // (serve_agent prints "refusing non-loopback bind" and returns),
            // which is the safe direction for a guard like this.
            "[::ffff:127.0.0.1]:9230", // IPv4-mapped: Ipv6Addr::is_loopback is
            // true only for ::1, so this is refused even though bind() would
            // land on the loopback interface.
            "[::1]", // no port; bind() rejects it anyway
        ] {
            assert!(!is_loopback_addr(addr), "{addr} must NOT count as loopback");
        }
    }

    /// An address read from a file or a `.env` commonly carries a trailing
    /// newline, and the parsers are strict. Failing closed there would silently
    /// refuse to start the endpoint on a perfectly good address.
    #[test]
    fn surrounding_whitespace_does_not_defeat_the_guard() {
        assert!(is_loopback_addr(" 127.0.0.1:9230"));
        assert!(is_loopback_addr("127.0.0.1:9230\n"));
        assert!(is_loopback_addr("\tlocalhost:9230 "));
        assert!(!is_loopback_addr(" 0.0.0.0:9230\n"));
    }

    /// `LUMEN_AGENT_TOKEN=` is `Ok("")`, not an error. Testing it with
    /// `is_err()` treated empty as CONFIGURED, which skipped the non-loopback
    /// refusal and then accepted `"auth": ""` from anyone —
    /// `LUMEN_AGENT_ADDR=0.0.0.0:9230 LUMEN_AGENT_TOKEN= ./app` published a
    /// tokenless remote-control socket.
    #[test]
    fn an_empty_token_counts_as_unset() {
        assert_eq!(normalize_token(None), None);
        assert_eq!(normalize_token(Some(String::new())), None);
        assert_eq!(normalize_token(Some("   ".into())), None);
        assert_eq!(normalize_token(Some("\n".into())), None);
        assert_eq!(
            normalize_token(Some("s3cret".into())),
            Some("s3cret".into())
        );
    }

    /// The bypass the old `starts_with` check allowed: a hostname whose text
    /// begins with `127.` or `localhost` is a different host entirely, and
    /// resolves wherever its DNS points. Treating it as loopback published a
    /// tokenless remote-control socket to the network.
    #[test]
    fn a_hostname_that_merely_looks_loopback_is_refused() {
        for addr in [
            "127.0.0.1.attacker.example:9230",
            "127.evil.test:9230",
            "localhost.attacker.example:9230",
            "[::1].evil:9230",
        ] {
            assert!(
                !is_loopback_addr(addr),
                "{addr} is a HOSTNAME, not the loopback interface — treating \
                 it as loopback exposes the agent socket with no token"
            );
        }
    }

    #[test]
    fn no_configured_token_leaves_the_endpoint_open() {
        // Only reachable on a loopback bind, which is the design.
        assert!(auth_ok(None, None));
        assert!(auth_ok(Some("anything"), None));
    }

    /// These pin a POLICY, not `PartialEq`: the failure this guards against is
    /// someone "helpfully" adding `.trim()` or `eq_ignore_ascii_case` to make a
    /// copy-paste token work, which silently widens what is accepted.
    #[test]
    fn a_configured_token_must_match_exactly() {
        assert!(auth_ok(Some("s3cret"), Some("s3cret")));
        assert!(
            !auth_ok(None, Some("s3cret")),
            "missing auth must be refused"
        );
        assert!(
            !auth_ok(Some(""), Some("s3cret")),
            "empty auth must be refused"
        );
        assert!(!auth_ok(Some("wrong"), Some("s3cret")));
        assert!(
            !auth_ok(Some("s3cret "), Some("s3cret")),
            "no trimming: a near-miss token must be refused"
        );
        assert!(
            !auth_ok(Some("S3CRET"), Some("s3cret")),
            "comparison must be case-sensitive"
        );
    }

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

// --- P.3b: SystemRequest fulfilment ----------------------------------------

/// Fulfil drained [`SystemRequest`]s (P.3b): `resolve` produces the reply
/// value for requests that have one (a file path for `OpenFile`); the reply
/// lands in the request's `reply` signal. Split from the rfd-backed resolver
/// so the delivery plumbing is unit-testable without a display.
pub fn fulfill_system_requests<
    R: lumen_render::Renderer,
    E: lumen_core::tasks::Spawner,
    P: lumen_widgets::app::PlatformConfig,
>(
    h: &mut Headless<R, E, P>,
    reqs: Vec<lumen_widgets::system::SystemRequest>,
    mut resolve: impl FnMut(&lumen_widgets::system::SystemRequest) -> Option<String>,
) {
    use lumen_widgets::system::SystemRequest;
    let mut delivered = false;
    for req in reqs {
        match &req {
            SystemRequest::OpenFile { reply, .. } => {
                if let Some(path) = resolve(&req) {
                    let sig: lumen_core::state::Signal<String> =
                        h.runtime().signal(reply, String::new);
                    sig.set(h.runtime(), path);
                    delivered = true;
                }
            }
            SystemRequest::Notification { title, body } => {
                // P.3e: native desktop notification, terminal fallback.
                if !notify_native(title, body) {
                    eprintln!("lumen notification: {title}: {body}");
                }
            }
            // Recorded-only until their fulfilment slice (P.3e).
            _ => {}
        }
    }
    if delivered {
        h.pump();
    }
}

/// P.3e: show a desktop notification. Linux dispatches through
/// `notify-send` (freedesktop spec; present on every desktop this targets —
/// a D-Bus client dep would drag an async runtime into the shell, the same
/// trade rejected for rfd's portal backend). Other platforms fall back to
/// the caller's terminal path until their shells land.
#[cfg(target_os = "linux")]
fn notify_native(title: &str, body: &str) -> bool {
    std::process::Command::new("notify-send")
        .arg("--app-name=Lumen")
        .arg("--")
        .arg(title)
        .arg(body)
        .spawn()
        .is_ok()
}

#[cfg(not(target_os = "linux"))]
fn notify_native(_title: &str, _body: &str) -> bool {
    false
}

/// The rfd-backed resolver: a modal native file-open dialog (GTK backend,
/// ADR-P1 P.3b decision — the portal backend needs a full async runtime).
///
/// GX2: without `desktop-integration` the whole GTK cluster is dropped and this
/// resolves nothing. The function still exists and still returns `Option`, so
/// the caller's contract is unchanged and "no native dialog available" is an
/// observable `None` rather than a build error or a silent no-op.
#[cfg(feature = "desktop-integration")]
pub fn native_dialog_resolver(req: &lumen_widgets::system::SystemRequest) -> Option<String> {
    if let lumen_widgets::system::SystemRequest::OpenFile { filters, .. } = req {
        let mut dlg = rfd::FileDialog::new();
        if !filters.is_empty() {
            let refs: Vec<&str> = filters.iter().map(String::as_str).collect();
            dlg = dlg.add_filter("files", &refs);
        }
        return dlg.pick_file().map(|p| p.display().to_string());
    }
    None
}

/// Stand-in for [`native_dialog_resolver`] when `desktop-integration` is off:
/// there is no native dialog backend linked, so every request is unresolved.
#[cfg(not(feature = "desktop-integration"))]
pub fn native_dialog_resolver(_req: &lumen_widgets::system::SystemRequest) -> Option<String> {
    None
}

#[cfg(test)]
mod system_fulfil_tests {
    use super::*;
    use lumen_core::state::Signal;
    use lumen_widgets::system::SystemRequest;
    use lumen_widgets::{col, widgets, App};

    #[test]
    fn bare_runtime_string_signal_roundtrip() {
        let rt = lumen_core::Runtime::new();
        let s: Signal<String> = rt.signal("x", String::new);
        s.set(&rt, "v".into());
        assert_eq!(s.get(&rt), "v");
    }

    #[test]
    fn open_file_reply_lands_in_the_named_signal() {
        let mut h = App::new(|_cx| col![widgets::text("app").id("t")])
            // The lean widget graph (T.4): this test executes signal set/get
            // under the lean State bound — the path that was silently broken
            // until it ran here (the Box-blanket dispatch bug, fixed in
            // lumen-core alongside this test).
            .with_executor(lumen_core::tasks::ThreadPoolSpawner::new(1))
            .run_headless(lumen_core::geometry::Size::new(200.0, 100.0));
        h.pump();

        fulfill_system_requests(
            &mut h,
            vec![SystemRequest::OpenFile {
                filters: vec!["png".into()],
                reply: "doc.path".into(),
            }],
            |_| Some("/tmp/pic.png".into()),
        );
        let sig: Signal<String> = h.runtime().signal("doc.path", String::new);
        assert_eq!(sig.get(h.runtime()), "/tmp/pic.png");
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;
    use lumen_widgets::system::{MenuItem, MenuModel};
    use lumen_widgets::{col, widgets, App};

    fn model() -> MenuModel {
        MenuModel {
            items: vec![MenuItem::submenu(
                "file",
                "File",
                vec![
                    MenuItem::new("file.open", "Open…").accel("Ctrl+O"),
                    {
                        let mut save = MenuItem::new("file.save", "Save").accel("Ctrl+Shift+S");
                        save.enabled = false;
                        save
                    },
                    MenuItem::new("file.quit", "Quit"),
                ],
            )],
        }
    }

    #[test]
    fn accel_chords_parse() {
        assert_eq!(
            parse_accel("Ctrl+O"),
            Some((Modifiers::CTRL, Key::Character("o".into())))
        );
        assert_eq!(
            parse_accel("ctrl+shift+s"),
            Some((
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::Character("s".into())
            ))
        );
        assert_eq!(
            parse_accel("Alt+Enter"),
            Some((Modifiers::ALT, Key::Named(NamedKey::Enter)))
        );
        // CmdOrCtrl resolves per-platform (Ctrl here on Linux/Windows).
        let (m, _) = parse_accel("CmdOrCtrl+Q").unwrap();
        assert!(m == Modifiers::CTRL || m == Modifiers::META);
        assert_eq!(parse_accel("Ctrl+Bogus"), None);
        assert_eq!(parse_accel("Ctrl"), None); // modifier with no key
    }

    #[test]
    fn accel_target_matches_enabled_items_only() {
        let m = model();
        assert_eq!(
            accel_target(&m, Modifiers::CTRL, &Key::Character("o".into())).as_deref(),
            Some("file.open")
        );
        // Case-insensitive on the character.
        assert_eq!(
            accel_target(&m, Modifiers::CTRL, &Key::Character("O".into())).as_deref(),
            Some("file.open")
        );
        // Wrong modifiers: no match.
        assert_eq!(
            accel_target(
                &m,
                Modifiers::CTRL | Modifiers::ALT,
                &Key::Character("o".into())
            ),
            None
        );
        // Disabled item never fires.
        assert_eq!(
            accel_target(
                &m,
                Modifiers::CTRL | Modifiers::SHIFT,
                &Key::Character("s".into())
            ),
            None
        );
    }

    /// GX2: the native menu only exists when the backend is linked.
    #[cfg(feature = "desktop-integration")]
    #[test]
    fn native_menu_mirrors_the_model() {
        // Executes muda's (GTK-backend) item construction on Linux — the
        // native tree must carry the same ids/labels/structure so a native
        // click reports the portable command id.
        let menu = build_native_menu(&model());
        let items = menu.items();
        assert_eq!(items.len(), 1);
        match &items[0] {
            muda::MenuItemKind::Submenu(s) => {
                assert_eq!(s.text(), "File");
                let kids = s.items();
                assert_eq!(kids.len(), 3);
                assert_eq!(kids[0].id().0, "file.open");
                assert_eq!(kids[2].id().0, "file.quit");
            }
            other => panic!("expected a submenu, got id {:?}", other.id()),
        }
    }

    #[test]
    fn accelerator_activates_menu_and_runs_the_bound_command() {
        // The full Linux activation path: chord → accel_target → activate_menu
        // → the `cx.register_command` handler under the same id.
        let mut h = App::new(|cx| {
            let n = cx.signal("n", || 0i32);
            cx.register_command("file.new", move |rt| n.update(rt, |v| *v += 1));
            col![widgets::text("app").id("t")]
        })
        .run_headless(lumen_core::geometry::Size::new(200.0, 100.0));
        h.pump();
        h.set_menu(MenuModel {
            items: vec![MenuItem::new("file.new", "New").accel("Ctrl+N")],
        });

        let id = accel_target(h.menu(), Modifiers::CTRL, &Key::Character("n".into()))
            .expect("chord resolves to the item");
        let label = h.activate_menu(&id);
        assert_eq!(label.as_deref(), Some("New"));
        assert_eq!(h.invoked_menu(), ["file.new"]);
        let n: lumen_core::state::Signal<i32> = h.runtime().signal("n", || 0);
        assert_eq!(n.get(h.runtime()), 1, "bound command ran");
    }
}

/// AT1: `route_at_action` is the only path a real screen-reader click takes
/// into the app, and until now nothing exercised it.
///
/// `crates/lumen-widgets/tests/a11y.rs` matches nodes by tree *position*, so it
/// stays green even if published ids stop resolving. And the function fails
/// **open** — `let Some(bounds) = … else { return; }` — so a broken lookup is a
/// silent no-op: the AT reports success, nothing happens, and no diagnostic is
/// emitted anywhere. That combination is why this needs a dedicated test before
/// ID1 changes how ids are minted.
#[cfg(test)]
mod at_routing_tests {
    use super::*;
    use lumen_widgets::{col, widgets, App, BuildCx, Element, Headless};

    /// Walk the published AccessKit tree for the node carrying `label`.
    fn published_id(update: &accesskit::TreeUpdate, label: &str) -> accesskit::NodeId {
        update
            .nodes
            .iter()
            .find(|(_, n)| n.label().is_some_and(|l| l == label))
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no published node labelled {label:?}"))
    }

    fn counter_app() -> Headless {
        App::new(|cx: &mut BuildCx| -> Element {
            let n = cx.signal("clicks", || 0i64);
            col![
                widgets::text(format!("clicks: {}", n.get(cx.runtime()))).id("out"),
                widgets::button("Bump", move |rt| n.update(rt, |v| *v += 1)).id("bump")
            ]
        })
        .run_headless(kurbo::Size::new(320.0, 200.0))
    }

    #[test]
    fn at_click_on_a_published_id_reaches_the_handler() {
        let mut h = counter_app();
        h.pump();

        // Take the id from the tree we actually publish to the platform, not
        // from an internal field — that is the whole point: it proves the id
        // an AT receives round-trips back to the right node.
        let update = lumen_widgets::a11y::build_tree(&h.semantics_elided());
        let target = published_id(&update, "Bump");

        route_at_action(
            &mut h,
            &accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            },
        );

        let n: lumen_core::state::Signal<i64> = h.runtime().signal("clicks", || 0);
        assert_eq!(
            n.get(h.runtime()),
            1,
            "an AT Click on the published id must invoke the button's handler"
        );
    }

    #[test]
    fn an_unresolvable_id_is_a_no_op_not_a_panic() {
        let mut h = counter_app();
        h.pump();

        // Failing open is the deliberate behavior (an AT must never crash the
        // app), which is exactly why the positive case above has to be tested:
        // this path cannot distinguish "no such node" from "ids broke".
        route_at_action(
            &mut h,
            &accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: accesskit::NodeId(u64::MAX),
                data: None,
            },
        );

        let n: lumen_core::state::Signal<i64> = h.runtime().signal("clicks", || 0);
        assert_eq!(n.get(h.runtime()), 0, "unknown target must change nothing");
    }

    #[test]
    fn published_ids_are_unique_within_one_update() {
        // The guard ID-0a specifies for ID1's fold64. Its value is catching an
        // id-derivation bug that maps distinct nodes together — which would
        // make AT clicks land on the wrong widget — not birthday collisions.
        let mut h = counter_app();
        h.pump();

        let update = lumen_widgets::a11y::build_tree(&h.semantics_elided());
        let mut seen = std::collections::HashSet::new();
        for (id, _) in &update.nodes {
            assert!(
                seen.insert(*id),
                "duplicate published AccessKit id {id:?} — two nodes would \
                 be indistinguishable to assistive tech"
            );
        }
        assert!(seen.len() >= 2, "expected a non-trivial tree, got {seen:?}");
    }
}

#[cfg(test)]
mod a11y_optout_tests {
    use super::a11y_enabled_from;

    /// GX4: the default must stay ON. Accessibility is a correctness property;
    /// a resource win that silently drops AT support would be a regression
    /// dressed as an optimization.
    #[test]
    fn defaults_to_enabled() {
        assert!(a11y_enabled_from(None, None));
    }

    #[test]
    fn lumen_variable_disables() {
        for v in ["0", "off", "false", "no", " 0 "] {
            assert!(!a11y_enabled_from(Some(v), None), "LUMEN_A11Y={v:?}");
        }
    }

    /// GTK/Qt already define this, so a user who disabled the AT bridge
    /// system-wide is obeyed without learning a Lumen-specific variable.
    #[test]
    fn no_at_bridge_disables() {
        assert!(!a11y_enabled_from(None, Some("1")));
        assert!(!a11y_enabled_from(None, Some("true")));
        // Only the truthy values disable: NO_AT_BRIDGE=0 means "use the bridge".
        assert!(a11y_enabled_from(None, Some("0")));
    }

    /// An explicit Lumen setting wins in BOTH directions, so an app that needs
    /// AT support can re-enable it under a system-wide `NO_AT_BRIDGE=1`.
    #[test]
    fn explicit_lumen_setting_overrides_no_at_bridge() {
        assert!(a11y_enabled_from(Some("1"), Some("1")));
        assert!(!a11y_enabled_from(Some("0"), Some("0")));
    }
}
