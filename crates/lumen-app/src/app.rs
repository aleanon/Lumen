//! The application and its headless runtime (02 §8).
//!
//! `Headless::pump` runs one turn: drain input → rebuild the element tree →
//! lay out → paint to the CPU renderer → build the semantic tree. It integrates
//! lumen-core (tree/state/events/semantics), lumen-layout, lumen-render, and
//! lumen-text. Interactive state (focus/hover) is keyed by [`StableId`] so it
//! survives the from-scratch rebuild.

use crate::element::{BuildCx, Element, Handler, NodeContent};
use kurbo::{Point, Rect, Size};
use lumen_core::events::{Event, InputQueue, Key, NamedKey, PointerState};
use lumen_core::identity::{IdHash, ScopePath};
use lumen_core::semantics::{
    Action, Role, SemanticsDoc, SemanticsNode, State as SemState, WindowInfo,
};
use lumen_core::state::{Runtime, SignalId};
use lumen_core::tree::{NodeFlags, Tree};
use lumen_core::{Color, NodeIndex, StableId};
use lumen_layout::{Dim, LayoutEngine, LayoutNode, LayoutStyle, LayoutTree};
use lumen_render::{
    cpu, BlendMode, Border, Brush, CornerRadii, Damage, DisplayList, DrawCmd, RgbaImage,
    RoundedRect,
};
// `Present` describes a swapchain hand-off, so its only users are the surface
// methods below — all of which carry this same cfg. Importing it unconditionally
// is an unused import in the lean profile, which is `-D warnings` in the `lean`
// CI job (LN0).
#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
use lumen_render::Present;
use lumen_text::{TextBlockApi, TextEngine, TextEngineApi};
use std::cell::RefCell;

use crate::fxhash::HashMap;
use std::rc::Rc;

/// Hit-test z for overlay subtrees (dropdown menus, popovers, tooltips). They
/// paint on top in a final pass, so they must also win hit-testing over the
/// normal-flow content they cover (which has the default z of 0).
const OVERLAY_Z: u32 = 1000;

/// MOD1: the platform bundle — one type parameter carrying every swappable
/// internal, instead of one parameter per seam.
///
/// # Why a bundle
///
/// `App<R, E, L, T, S, …>` would add a type parameter per seam. On a 4,600-line
/// runtime that costs build time on every edit, strains coherence, and forces
/// every downstream signature (`Headless<R, E, …>` appears across three crates)
/// to grow with the seam count. A bundle keeps the arity fixed: adding a seam
/// later adds an associated type here, not a parameter everywhere.
///
/// # What it does not do yet
///
/// The seams it names — [`LayoutEngine`](lumen_layout::LayoutEngine) and
/// [`TextEngineApi`](lumen_text::TextEngineApi) — are verified substitutable by
/// their own `engine_seam.rs` tests. What this trait adds is the *selection*
/// mechanism. The `Style` seam is deliberately absent: MOD4's extension point is
/// runtime registration (`lumen_style::register_property`), not a type swap, so
/// there is nothing for an associated type to name.
///
/// `Default` is required on both because the runtime constructs a fresh layout
/// tree per rebuild and one text engine per window; a bundle whose members
/// could not be constructed without arguments would need a factory method and a
/// stored instance, which is more machinery than the seam warrants today.
/// MOD7 S3: the memory-vs-speed knobs, as data rather than as more traits.
///
/// These were hardcoded `const`s with no seam at all, which made "tune this app
/// for low memory" unreachable by any mechanism — a type parameter is the wrong
/// tool for a number, and a Cargo feature cannot express "a quarter of the
/// default". Carried on [`PlatformConfig`] so it composes with the bundle
/// instead of competing with it.
///
/// **Only genuinely per-app caches are here.** The glyph bitmap cache is
/// `thread_local` and shared by every engine on the thread, and the image and
/// animation caches are process-global statics; a per-app knob for any of them
/// would read as configuration and behave as a race, so they stay constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuning {
    /// Starting ceiling for the shaped-block cache. Both text caches grow
    /// adaptively up to a hard cap, so this sets where growth begins.
    pub shape_cache_cap: usize,
    /// Starting ceiling for the glyph-run cache.
    pub run_cache_cap: usize,
}

impl Tuning {
    /// Today's shipped values — what every app got before this existed.
    pub const DEFAULT: Tuning = Tuning {
        shape_cache_cap: 2048,
        run_cache_cap: 4096,
    };

    /// A quarter of [`DEFAULT`](Tuning::DEFAULT): trades re-shaping work for
    /// resident memory. Sensible for a small or embedded app whose text is a
    /// handful of labels rather than a document.
    pub const LEAN: Tuning = Tuning {
        shape_cache_cap: 512,
        run_cache_cap: 1024,
    };
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning::DEFAULT
    }
}

/// MOD1: the swappable internals a runtime is built on — the layout engine
/// (MOD2) and the text engine (MOD3) — named together so an app selects a
/// bundle rather than a list of parameters.
///
/// `Default` is required on both because the runtime constructs a fresh layout
/// tree per rebuild and one text engine per window; a bundle whose members
/// could not be constructed without arguments would need a factory method and a
/// stored instance, which is more machinery than the seam warrants today.
/// [`AppConfig`] takes the other road for renderer and executor, where the
/// factory buys the `Box<dyn Renderer>` case.
pub trait PlatformConfig: 'static {
    /// MOD7 S3: cache ceilings for this bundle. Defaulted, so an existing
    /// `impl PlatformConfig` keeps the shipped values without naming them.
    const TUNING: Tuning = Tuning::DEFAULT;

    /// The layout engine (MOD2).
    type Layout: lumen_layout::LayoutEngine + Default;
    /// The text engine (MOD3).
    type Text: lumen_text::TextEngineApi + Default;
}

/// MOD7 S2: one name for all four swap axes — renderer, executor, layout and
/// text — so a consumer writes `ConfiguredApp<MyConfig>` instead of naming
/// three type parameters.
///
/// This is deliberately **additive**. `App<R, E, P>` keeps its three
/// parameters, because varying one axis (`with_renderer`, the shell's own
/// `Box<dyn Renderer>`) is a real use and a single fused parameter would force
/// a whole new config to change one thing. `AppConfig` is the ergonomic entry
/// point on top; neither replaces the other.
///
/// Renderer and executor arrive through **factory functions** rather than a
/// `Default` bound, which is what lets a config name `Box<dyn Renderer>` — the
/// shape the shell itself uses, and one that cannot implement `Default`.
/// [`PlatformConfig`] took the other road (a `Default` bound) because a layout
/// tree and a text engine are constructed per rebuild and per window, where a
/// stored factory would be machinery the seam does not warrant.
pub trait AppConfig: 'static {
    /// The frame renderer (MOD-R).
    type Renderer: lumen_render::Renderer;
    /// The background-work executor.
    type Executor: lumen_core::tasks::Spawner;
    /// The layout engine (MOD2).
    type Layout: lumen_layout::LayoutEngine + Default;
    /// The text engine (MOD3).
    type Text: lumen_text::TextEngineApi + Default;

    /// Construct the renderer. Called once per app.
    fn renderer() -> Self::Renderer;
    /// Construct the executor. Called once per app.
    fn executor() -> Self::Executor;
}

/// The [`PlatformConfig`] half of an [`AppConfig`], so the existing
/// `App<R, E, P>` machinery can carry a fused config without changing shape.
pub struct PlatformOf<C>(std::marker::PhantomData<C>);

impl<C: AppConfig> PlatformConfig for PlatformOf<C> {
    type Layout = C::Layout;
    type Text = C::Text;
}

/// An [`App`] fully described by one [`AppConfig`] — the `Runtime<MyConfig>`
/// shape, spelled as a type alias so the three parameters underneath stay
/// available.
pub type ConfiguredApp<C> =
    App<<C as AppConfig>::Renderer, <C as AppConfig>::Executor, PlatformOf<C>>;

/// A [`Headless`] fully described by one [`AppConfig`].
pub type ConfiguredHeadless<C> =
    Headless<<C as AppConfig>::Renderer, <C as AppConfig>::Executor, PlatformOf<C>>;

/// The shipped bundle: taffy layout (ADR-004) + parley/swash text (ADR-005).
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultPlatform;

impl PlatformConfig for DefaultPlatform {
    type Layout = LayoutTree;
    type Text = TextEngine;
}

/// MOD7 S4: the shipped presets, so the common cases are one word and a custom
/// [`AppConfig`] is the escape hatch rather than the entry fee.
///
/// All three use the shipped engines — they differ in the choices *around*
/// them, which is what an app actually picks between. Swapping an engine is a
/// custom config, because there is no second implementation to name.
///
/// ```ignore
/// ConfiguredApp::<Desktop>::with_config(view).run(size);
/// ```
pub mod presets {
    use super::{AppConfig, PlatformConfig, Tuning};
    use crate::app::{DefaultPlatform, LayoutTree, TextEngine};

    /// Smallest resident footprint: the CPU reference renderer, no background
    /// threads, and quartered text caches. The inline executor runs
    /// `cx.task`/`cx.resource` work on the caller's thread, so this is for
    /// apps that do little or none — it is also the deterministic one, which
    /// is why the test harnesses use its shape.
    pub struct Lean;

    /// The shipped defaults, named: CPU reference renderer, a small thread
    /// pool, default caches. What `App::new` has always given you.
    pub struct Balanced;

    /// A desktop app: GPU-capable boxed renderer chosen at startup, a
    /// four-worker pool, default caches. The renderer is boxed because the
    /// shell picks GPU-or-CPU by adapter presence, which no static type can
    /// express.
    pub struct Desktop;

    /// The bundle all three share; `Lean` overrides only its tuning.
    pub struct LeanPlatform;

    impl PlatformConfig for LeanPlatform {
        type Layout = LayoutTree;
        type Text = TextEngine;
        const TUNING: Tuning = Tuning::LEAN;
    }

    impl AppConfig for Lean {
        type Renderer = lumen_render::TinySkia;
        type Executor = lumen_core::tasks::InlineSpawner;
        type Layout = <LeanPlatform as PlatformConfig>::Layout;
        type Text = <LeanPlatform as PlatformConfig>::Text;
        fn renderer() -> Self::Renderer {
            lumen_render::TinySkia
        }
        fn executor() -> Self::Executor {
            lumen_core::tasks::InlineSpawner
        }
    }

    impl AppConfig for Balanced {
        type Renderer = lumen_render::TinySkia;
        type Executor = lumen_core::tasks::ThreadPoolSpawner;
        type Layout = <DefaultPlatform as PlatformConfig>::Layout;
        type Text = <DefaultPlatform as PlatformConfig>::Text;
        fn renderer() -> Self::Renderer {
            lumen_render::TinySkia
        }
        fn executor() -> Self::Executor {
            lumen_core::tasks::ThreadPoolSpawner::new(2)
        }
    }

    impl AppConfig for Desktop {
        type Renderer = Box<dyn lumen_render::Renderer>;
        type Executor = lumen_core::tasks::ThreadPoolSpawner;
        type Layout = <DefaultPlatform as PlatformConfig>::Layout;
        type Text = <DefaultPlatform as PlatformConfig>::Text;
        fn renderer() -> Self::Renderer {
            Box::new(lumen_render::TinySkia)
        }
        fn executor() -> Self::Executor {
            lumen_core::tasks::ThreadPoolSpawner::new(4)
        }
    }
}

/// O1.3: the per-frame budget a painted frame is measured against — one 60 Hz
/// vsync interval. Frames past it are counted (`frames_over_budget`) so an
/// agent can see jank that rolling percentiles hide.
///
/// A fixed 60 Hz rather than the display's actual rate: this is a diagnostic
/// yardstick, not a scheduling deadline, and it must mean the same thing across
/// machines for a reported number to be comparable.
pub const FRAME_BUDGET_MS: f32 = 1000.0 / 60.0;

/// One in-flight animation (O3.3, `ui.animations`).
#[derive(Clone, Debug)]
pub struct AnimationInfo {
    /// The animating node's author id.
    pub node: String,
    /// The property being animated, or `"animation"` for a keyframe timeline.
    pub property: &'static str,
    /// `0.0..=1.0` for a transition; `0.0` for a keyframe timeline, whose
    /// phase is not a fraction of a finite whole.
    pub progress: f64,
    /// Milliseconds until this transition's declared duration elapses.
    pub remaining_ms: f64,
    /// Whether this animation has no declared end (`animation: … infinite`).
    /// Such an animation is **never** overdue — it is working as declared.
    pub infinite: bool,
    /// How far past its declared total a finite animation has run. `0.0` while
    /// it is on time.
    pub overdue_ms: f64,
}

/// O1.3: the performance surface behind `app.perf`.
///
/// **Counters are cumulative for the life of the runtime**, not per-frame.
/// Bracket an interaction with two reads and subtract. The per-frame variants
/// live on [`FrameStats`], which `pump` returns — they cannot serve the agent,
/// because `ui.waitSettled` ends on idle pumps that zero them.
#[derive(Clone, Copy, Debug)]
pub struct PerfReport {
    /// Median painted-frame time (ms) over the last 120 painted frames.
    pub frame_ms_p50: f64,
    /// 95th-percentile painted-frame time (ms) over the same window.
    pub frame_ms_p95: f64,
    /// Worst painted frame (ms) since start — **all-time, not windowed**, so a
    /// single stall stays visible after the percentiles have forgotten it.
    pub frame_ms_max: f64,
    /// Painted frames since start.
    pub frames_rendered: u64,
    /// Painted frames that exceeded [`FRAME_BUDGET_MS`], since start.
    pub frames_over_budget: u64,
    /// The budget the count above is measured against (ms).
    pub frame_budget_ms: f64,
    /// Nodes rebuilt from scratch, since start.
    pub nodes_rebuilt_total: u64,
    /// Nodes copied forward by the retained pipeline, since start. A copy rate
    /// near zero against a large rebuild count means memoization is not paying.
    pub nodes_copied_total: u64,
    /// Style-memo hits since start.
    pub style_memo_hits: u64,
    /// Style-memo misses since start.
    pub style_memo_misses: u64,
    /// Shaped-text cache occupancy and its current (retargeted) soft cap.
    /// `len` approaching `cap` repeatedly is the text-thrash leading indicator.
    pub shape_cache_len: usize,
    /// Current soft cap of the shaped-text cache.
    pub shape_cache_cap: usize,
    /// Glyph-run cache occupancy.
    pub run_cache_len: usize,
    /// Current soft cap of the glyph-run cache.
    pub run_cache_cap: usize,
    /// Active renderer's name — one field that answers "why is this slow".
    pub renderer: &'static str,
    /// Whether that renderer is GPU-backed. A silent CPU fallback is an
    /// order-of-magnitude difference an agent cannot otherwise detect.
    pub is_gpu: bool,
    /// The graphics backend in use (`"Vulkan"`, `"Gl"`, `"cpu"`, …).
    pub backend: &'static str,
    /// Whether that backend is known to render some content wrongly (today:
    /// GL silently drops every gradient). Queryable rather than log-only,
    /// because the W0115 advisory is drained by the first painted frame.
    pub backend_has_known_defects: bool,
}

/// Statistics for one rendered frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameStats {
    /// Number of live nodes after the rebuild.
    pub node_count: usize,
    /// Whether any pixels were repainted this frame (`false` = idle frame, the
    /// previous frame was reused verbatim).
    pub painted: bool,
    /// What changed this frame (R2): `None` (idle), `Region` (only a rectangle
    /// repainted), or `Full`. The shell can upload just the changed region.
    pub damage: Damage,
    /// Nodes lowered fresh by the last rebuild (A.3.2) — the O(changed) meter.
    /// `0` when the pump was idle or patch-only.
    pub nodes_rebuilt: u32,
    /// Nodes whose retained work was copied forward from the previous build
    /// instead of being re-lowered (A.3.2 memo-hit spans).
    pub nodes_copied: u32,
}

/// P.3d: a declared secondary window — its descriptor plus its root closure.
type WindowDecl = (
    crate::system::WindowDesc,
    std::rc::Rc<dyn Fn(&mut BuildCx) -> Element>,
);

/// An application: a root build closure, an optional stylesheet, and the frame
/// renderer backend `R` (defaults to [`lumen_render::DefaultRenderer`] = the
/// deterministic CPU `TinySkia`). The runtime is generic over `R` — zero-cost by
/// default; a consumer who wants dynamic backend selection uses
/// `R = Box<dyn Renderer>` (see the blanket `Renderer` impl in `lumen-render`).
pub struct App<
    R = lumen_render::DefaultRenderer,
    E = lumen_core::tasks::InlineSpawner,
    P = DefaultPlatform,
> {
    /// MOD1: `App` names the platform bundle only to hand it to the `Headless`
    /// it builds — it holds no layout or text state itself.
    _platform: std::marker::PhantomData<P>,
    /// The root view.
    ///
    /// Returns a boxed `DirectDyn` rather than an `Element`: the *root* is the
    /// one place a box is unavoidable, because the closure's return type is
    /// erased by storage, and it is one box per frame rather than one per node.
    root: RootView,
    #[allow(dead_code)]
    stylesheet: Option<String>,
    /// Extra fonts to register at boot (B1): app-provided bytes, selected by
    /// family name via `TextStyle::family`. The bundled font stays the default.
    fonts: Vec<Vec<u8>>,
    /// P.3d: declared secondary windows — geometry/title plus each window's
    /// own root closure. Realized as separate [`Headless`] instances sharing
    /// this app's `Runtime` (`Headless::open_window`); the shell opens one
    /// OS window per declaration.
    windows: Vec<WindowDecl>,
    renderer: R,
    executor: E,
}

impl App<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner> {
    /// Create an app from its root build closure (02 §8), on the default CPU
    /// reference renderer and the deterministic inline executor.
    pub fn new(root: impl Fn(&mut BuildCx) -> Element + 'static) -> App {
        App::view(root)
    }

    /// An app whose root view returns **anything [`Direct`]**.
    ///
    /// [`new`](Self::new) is this with `V = Element`, and stays that way on
    /// purpose. Making `new` itself generic is a source-breaking change even
    /// though it accepts strictly more: a view body ending in `.into()` has no
    /// unique target type once more than one type is `Direct`, and ~25 call
    /// sites across this repo alone stopped inferring. An additive door costs
    /// nothing and breaks nobody.
    pub fn view<V: Direct + 'static>(root: impl Fn(&mut BuildCx) -> V + 'static) -> App {
        App {
            _platform: std::marker::PhantomData,
            root: Box::new(move |cx| Box::new(Some(root(cx)))),
            stylesheet: None,
            fonts: Vec::new(),
            windows: Vec::new(),
            renderer: lumen_render::TinySkia,
            executor: lumen_core::tasks::InlineSpawner,
        }
    }
}

impl<P: PlatformConfig> App<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner, P> {
    /// MOD1: like [`App::new`](App::new), but on a chosen [`PlatformConfig`]
    /// instead of the shipped taffy + parley bundle.
    ///
    /// A separate constructor rather than a generic `new`, because `App::new`
    /// must keep inferring its parameters from nothing — a struct's type-
    /// parameter defaults do not apply to inference of a function's return, so
    /// generalising `new` would force every existing call site to name a
    /// platform.
    pub fn with_platform<V: Direct + 'static>(root: impl Fn(&mut BuildCx) -> V + 'static) -> Self {
        App {
            _platform: std::marker::PhantomData,
            root: Box::new(move |cx| Box::new(Some(root(cx))) as Box<dyn DirectDyn>),
            stylesheet: None,
            fonts: Vec::new(),
            windows: Vec::new(),
            renderer: lumen_render::TinySkia,
            executor: lumen_core::tasks::InlineSpawner,
        }
    }
}

impl<C: AppConfig> ConfiguredApp<C> {
    /// MOD7 S2: build an app from one [`AppConfig`] — the `Runtime<MyConfig>`
    /// entry point.
    ///
    /// ```ignore
    /// struct Lean;
    /// impl AppConfig for Lean {
    ///     type Renderer = TinySkia;
    ///     type Executor = InlineSpawner;
    ///     type Layout   = LayoutTree;
    ///     type Text     = MyTinyTextEngine;
    ///     fn renderer() -> TinySkia { TinySkia }
    ///     fn executor() -> InlineSpawner { InlineSpawner }
    /// }
    /// ConfiguredApp::<Lean>::with_config(view).run(size);
    /// ```
    ///
    /// Named `with_config` rather than made a generic `new` for the reason
    /// `with_platform` records: a struct's type-parameter defaults do not apply
    /// to inference of a function's *return*, so generalising `new` would force
    /// every existing call site to name a config.
    pub fn with_config(root: impl Fn(&mut BuildCx) -> Element + 'static) -> Self {
        App {
            _platform: std::marker::PhantomData,
            root: Box::new(move |cx| Box::new(Some(root(cx))) as Box<dyn DirectDyn>),
            stylesheet: None,
            fonts: Vec::new(),
            windows: Vec::new(),
            renderer: C::renderer(),
            executor: C::executor(),
        }
    }
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig> App<R, E, P> {
    /// Attach a stylesheet (parsed in M1; stored for now).
    pub fn stylesheet(mut self, lss: &str) -> Self {
        self.stylesheet = Some(lss.to_string());
        self
    }

    /// Register an extra font (its bytes) for the app, selectable by family name
    /// via [`TextStyle::family`](lumen_text::TextStyle::family). Additive — the
    /// bundled font stays the default; no system-font enumeration (ADR-005).
    pub fn with_font(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.fonts.push(bytes.into());
        self
    }

    /// P.3d: declare a secondary window — its descriptor (id/title/size)
    /// and its own root build closure. The window shares the app's reactive
    /// store: a signal written in one window re-renders every window that
    /// reads it.
    pub fn window(
        mut self,
        desc: crate::system::WindowDesc,
        root: impl Fn(&mut BuildCx) -> Element + 'static,
    ) -> Self {
        self.windows.push((desc, std::rc::Rc::new(root)));
        self
    }

    /// Swap the frame renderer backend, changing the app's `R` type (typestate
    /// builder). The CPU reference renderer is the default; the shell hands in a
    /// GPU backend (constructed post-surface), and a consumer wanting runtime
    /// selection passes a `Box<dyn Renderer>`.
    ///
    /// MOD7 S0: the return type names `P`. It used to be `App<R2, E>`, which
    /// defaulted the third parameter — so calling this on an app built with
    /// [`with_platform`](App::with_platform) silently reverted it to
    /// `DefaultPlatform`, and the app ran on the shipped text and layout
    /// engines instead of the chosen ones. It type-errors only if the caller
    /// annotates the result, which is why nothing caught it; the guard is
    /// `lumen-widgets/tests/platform_builder.rs`.
    pub fn with_renderer<R2: lumen_render::Renderer>(self, renderer: R2) -> App<R2, E, P> {
        App {
            _platform: std::marker::PhantomData,
            root: self.root,
            stylesheet: self.stylesheet,
            fonts: self.fonts,
            windows: self.windows,
            renderer,
            executor: self.executor,
        }
    }

    /// Swap the background-work executor, changing the app's `E` type (typestate
    /// builder). Defaults to the deterministic [`InlineSpawner`](lumen_core::tasks::InlineSpawner);
    /// the shell hands in a real thread-pool / async executor, and a consumer
    /// wanting runtime selection passes a `Box<dyn Spawner>`.
    /// MOD7 S0: names `P` for the same reason `with_renderer` does.
    pub fn with_executor<E2: lumen_core::tasks::Spawner>(self, executor: E2) -> App<R, E2, P> {
        App {
            _platform: std::marker::PhantomData,
            root: self.root,
            stylesheet: self.stylesheet,
            fonts: self.fonts,
            windows: self.windows,
            renderer: self.renderer,
            executor,
        }
    }

    /// Run headless at `size` (no OS dependencies).
    pub fn run_headless(self, size: Size) -> Headless<R, E, P> {
        let mut h = self.into_headless(size, None);
        h.rebuild();
        h
    }

    /// Run headless, restoring a prior [`AppSnapshot`] (tier-3 restart,
    /// ADR-011). Returns the instance plus any `W0002` drop diagnostics raised
    /// when a snapshot value no longer has a matching signal. Snapshot builds
    /// only.
    #[cfg(feature = "snapshot")]
    pub fn run_headless_restored(
        self,
        size: Size,
        snap: AppSnapshot,
    ) -> (Headless<R, E, P>, Vec<lumen_core::Diagnostic>) {
        // Focus is host state (not in the reactive store), so it is carried on
        // the snapshot and re-applied directly.
        let mut h = self.into_headless(size, snap.focused.clone());
        // Stage the snapshot *before* the first build so each signal adopts its
        // restored value as it is (re-)created (Checkpoint protocol).
        h.rt.load_pending(snap.state);
        h.rebuild();
        let diags = h.rt.finish_restore();
        (h, diags)
    }

    /// Construct the headless instance (fonts registered, focus applied) without
    /// the first build. Shared by the plain and restore boot paths.
    fn into_headless(self, size: Size, focused: Option<StableId>) -> Headless<R, E, P> {
        // Register app fonts before the first build so styled text can select
        // them. Bytes are retained so secondary windows (P.3d) can build
        // their own TextEngine with the same faces.
        let font_bytes = self.fonts.clone();
        // MOD1: the bundle's text engine, not the concrete one. `Default` is
        // the constructor the seam offers (see `PlatformConfig`).
        let mut text = P::Text::default();
        // MOD7 S3: the bundle's cache ceilings, applied before first use.
        text.set_cache_caps(P::TUNING.shape_cache_cap, P::TUNING.run_cache_cap);
        for bytes in self.fonts {
            text.register_font(bytes);
        }
        let window_descs: Vec<crate::system::WindowDesc> =
            self.windows.iter().map(|(d, _)| d.clone()).collect();
        let stylesheet_src = self.stylesheet.clone();
        let h = Headless {
            root: self.root,
            window_decls: self.windows,
            font_bytes,
            stylesheet_src,
            rt: Runtime::new(),
            size,
            scale: 1.0,
            clock_ms: 0.0,
            renderer: self.renderer,
            executor: self.executor,
            task_waker: None,
            text,
            text_cache: HashMap::default(),
            shadow_cache: HashMap::default(),
            tree: Tree::new(),
            meta: HashMap::default(),
            #[cfg(feature = "dev-observability")]
            node_ink: HashMap::default(),
            node_caret: HashMap::default(),
            #[cfg(feature = "dev-observability")]
            node_text_metrics: HashMap::default(),
            frame: RgbaImage::new(size.width as u32, size.height as u32),
            sem_root: RefCell::new(None),
            handle_index: RefCell::new(None),
            build_panic: None,
            focused_id: focused,
            hovered_id: None,
            pressed: None,
            pending_click: None,
            pan: None,
            pan_vel: (0.0, 0.0, 0.0),
            fling: None,
            fling_ms: 0.0,
            input: InputQueue::new(),
            pointer: PointerState::new(),
            requests: crate::element::FrameRequests::default(),
            app_sheet: self.stylesheet.as_deref().and_then(parse_sheet),
            theme: lumen_style::ThemeKind::Light,
            node_style: HashMap::default(),
            node_computed: HashMap::default(),
            style_env: None,
            scope_spans: HashMap::default(),
            prev_spans: HashMap::default(),
            layout_scratch: P::Layout::default(),
            layout_reuse: false,
            allow_copy_forward: false,
            impure_seen: 0,
            nodes_rebuilt: 0,
            nodes_copied: 0,
            nodes_rebuilt_total: 0,
            nodes_copied_total: 0,
            style_memo: HashMap::default(),
            shaped_for_indefinite: 0,
            style_memo_hits: 0,
            style_memo_misses: 0,
            commands: HashMap::default(),
            prop_anims: HashMap::default(),
            reduced_motion: false,
            key_anims: HashMap::default(),
            theme_anim_until: 0.0,
            desc_stack: Vec::new(),
            desc_hash_stack: std::cell::RefCell::new(vec![Some(
                lumen_core::identity::IdHasher::new(),
            )]),
            container_nodes: Vec::new(),
            container_prev: Vec::new(),
            container_stack: Vec::new(),
            container_repass: false,
            hidden_count: 0,
            disabled_count: 0,
            #[cfg(feature = "dev-observability")]
            audit_diff: lumen_core::observe::FrameDiff::new(),
            #[cfg(feature = "dev-observability")]
            sem_gen: std::cell::Cell::new(0),
            #[cfg(feature = "dev-observability")]
            last_audit_gen: u64::MAX,
            #[cfg(feature = "dev-observability")]
            last_audit_ms: f64::NEG_INFINITY,
            last_paint_damage: (Damage::None, 0),
            frame_ms: std::collections::VecDeque::new(),
            frame_ms_max: 0.0,
            frames_over_budget: 0,
            frames_rendered: 0,
            menu: crate::system::MenuModel::default(),
            menu_rev: 0,
            invoked_menu: Vec::new(),
            system_requests: Vec::new(),
            windows: window_descs,
            rtl: false,
            last_dl: None,
            last_damage: lumen_render::Damage::Full,
            surface_attached: false,
            last_build_gen: 0,
            force_rebuild: false,
            last_build_clock: 0.0,
            scope_cache: RefCell::new(HashMap::default()),
            scope_live: RefCell::new(crate::fxhash::HashSet::default()),
            scope_skipped: RefCell::new(crate::fxhash::HashSet::default()),
            tasks_table: RefCell::new(HashMap::default()),
            bg_bindings: Vec::new(),
            binding_index: HashMap::default(),
            dl_patch: HashMap::default(),
            text_bindings: Vec::new(),
            structural_reads: lumen_core::state::ReadSet::default(),
            elided_cache: RefCell::new(None),
            #[cfg(feature = "snapshot")]
            json_cache: RefCell::new([None, None]),
            #[cfg(feature = "snapshot")]
            last_change: ChangeReport {
                kind: "idle",
                nodes: Vec::new(),
            },
        };
        h
    }
}

/// A tier-3 snapshot of a running app: the reactive store (every signal —
/// including scroll offsets) plus focus. Serializable, so it can be written
/// before a process restart and restored afterwards (ADR-011). Snapshot builds
/// only.
#[cfg(feature = "snapshot")]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSnapshot {
    state: lumen_core::state::StateSnapshot,
    focused: Option<StableId>,
}

/// Checkpoint protocol (02 §4, ADR-011) — the tier-2/3 hot-reload contract,
/// formalized over the snapshot machinery (W.4b). `quiesce` parks the app at
/// a safe point (reactive graph at fixpoint), `serialize_state` captures the
/// store + host extras (focus), `restore_state` adopts a snapshot into the
/// **running** instance (existing signals restored in place, re-created ones
/// adopt on rebuild; returns `W0002` drop diagnostics), and `resume` repaints
/// from the restored state. Snapshot builds only.
#[cfg(feature = "snapshot")]
pub trait Checkpoint {
    /// Park at a safe point: drain scheduled reactive work to a fixpoint.
    fn quiesce(&mut self);
    /// Capture the entire store plus host extras (focus).
    fn serialize_state(&self) -> AppSnapshot;
    /// Adopt `snap` into the running instance, returning `W0002` diagnostics
    /// for snapshot values that no longer have a matching signal.
    fn restore_state(&mut self, snap: AppSnapshot) -> Vec<lumen_core::Diagnostic>;
    /// Resume presentation: repaint from the restored state.
    fn resume(&mut self);
}

#[cfg(feature = "snapshot")]
impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig> Checkpoint
    for Headless<R, E, P>
{
    fn quiesce(&mut self) {
        // pump() flushes writes and asserts the graph is quiescent on exit.
        self.pump();
    }

    fn serialize_state(&self) -> AppSnapshot {
        self.snapshot()
    }

    fn restore_state(&mut self, snap: AppSnapshot) -> Vec<lumen_core::Diagnostic> {
        self.rt.load_pending(snap.state);
        // Existing slots adopt in place (schedules their subscribers) …
        let mut diags = self.rt.adopt_pending_live();
        // … focus is host state, re-applied directly …
        self.focused_id = snap.focused;
        // … and a forced rebuild lets conditionally-created signals adopt the
        // rest before leftovers become W0002 drops.
        self.force_rebuild = true;
        self.pump();
        diags.extend(self.rt.finish_restore());
        diags
    }

    fn resume(&mut self) {
        self.force_full_repaint();
    }
}

/// Parse a stylesheet, returning it only if error-free.
fn parse_sheet(src: &str) -> Option<lumen_style::Stylesheet> {
    let (sheet, diags) = lumen_style::parse("app.lss", src);
    (!lumen_style::has_errors(&diags)).then_some(sheet)
}

/// The result of a tier-1 hot reload (03 §3 reload event).
#[derive(Clone, Debug)]
pub enum ReloadResult {
    /// The stylesheet applied; styles changed live.
    Ok,
    /// The edit was rejected; the previous stylesheet stays live.
    Failed(Vec<lumen_core::Diagnostic>),
}

/// A retained paint-only prop binding (F3.4): its node, the binding, and the
/// signals it last read. When those change, the runtime re-evaluates the binding
/// and patches `meta[node]` + repaints — no rebuild, no relayout.
struct BoundBg {
    node: NodeIndex,
    dynamic: lumen_core::Dynamic<Color>,
    deps: lumen_core::state::ReadSet,
}

/// A retained text binding (F3.5): its node, the binding, the signals it last
/// read, and everything needed to decide whether re-evaluating it can change
/// layout.
///
/// Text was classified structural because a new string can measure to a new
/// size. That is true, but it is a property of the *value*, not of the binding:
/// most text updates keep the box exactly as it was — a label inside a sized
/// container, a wrapping paragraph that still fills the same number of lines,
/// a virtual-list row with a fixed item height. Those can patch like a
/// background does, at ~15x less cost than a rebuild.
///
/// So the binding remembers what the build measured, and the patch path
/// re-measures and compares. Same size ⇒ the node's `LayoutStyle` would come
/// out identical ⇒ no relayout is possible ⇒ patch. Different size ⇒ fall back
/// to a rebuild, which is always correct.
/// MUT1: a reference into one of the two binding tables, for the reverse
/// index (`binding_index`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum BindingSlot {
    Text(u32),
    Bg(u32),
}

/// MUT2: a bound node's footprint in the retained display list. `text_cmd` /
/// `bg_cmd` index the command `paint_patched` rewrites in place; `ineligible`
/// marks a node that emitted something string-dependent beyond the run itself
/// (an ellipsized display string, an editor caret/selection, a text
/// decoration) — a patch touching such a node falls back to a full `paint()`.
#[derive(Clone, Copy, Default)]
struct DlSlot {
    text_cmd: Option<u32>,
    bg_cmd: Option<u32>,
    ineligible: bool,
}

struct BoundText {
    node: NodeIndex,
    dynamic: lumen_core::Dynamic<String>,
    deps: lumen_core::state::ReadSet,
    /// The wrap width the build measured with — `None` for an unwrapped label.
    /// Re-measuring with anything else would compare two different questions.
    wrap: Option<f32>,
    /// Whether the measurement actually fed the layout style on that axis. An
    /// axis the author fixed cannot move no matter what the new string
    /// measures, so it is not compared.
    auto_w: bool,
    auto_h: bool,
    /// The measured block size, ceiled exactly as the sizing code ceils it.
    w: f32,
    h: f32,
    /// False when the node ellipsizes: the painted string is then a *derived*
    /// truncation, not the binding's value, and reproducing it here would mean
    /// duplicating that logic. Such a node always rebuilds.
    patchable: bool,
    /// T2: the label was laid out without shaping — width parent-assigned,
    /// height from line metrics — so no measurement exists to compare and none
    /// is needed. A replacement is layout-neutral iff it is still a single
    /// line; `w`/`h` are 0 and never consulted.
    deferred: bool,
}

/// Per-node reactive dependencies, split by source (F4). The union projects to
/// `SemanticsNode.deps` (F2); the breakdown backs `ui.getDeps` and the reverse
/// index. `background` deps update via a paint-only patch; `scope` and `class`
/// via a rebuild. `text` used to be in that second group and is no longer
/// (F3.5): it patches when the new string measures the same size, and rebuilds
/// only when the box would actually move.
#[cfg(feature = "dev-observability")]
#[derive(Default, Clone)]
struct NodeDeps {
    scope: Vec<String>,
    text: Vec<String>,
    background: Vec<String>,
    class: Vec<String>,
}

#[cfg(feature = "dev-observability")]
impl NodeDeps {
    /// De-duplicated union of all sources (for `SemanticsNode.deps`).
    fn union(&self) -> Vec<String> {
        let mut d: Vec<String> = Vec::new();
        for k in self
            .scope
            .iter()
            .chain(&self.text)
            .chain(&self.background)
            .chain(&self.class)
        {
            if !d.contains(k) {
                d.push(k.clone());
            }
        }
        d
    }

    fn is_empty(&self) -> bool {
        self.scope.is_empty()
            && self.text.is_empty()
            && self.background.is_empty()
            && self.class.is_empty()
    }
}

/// A reverse-index entry (F4.2): a node that depends on some signal, and how it
/// would update when that signal changes.
#[derive(Clone)]
/// Snapshot-only: `dependents_of` is the sole constructor, and it is the only
/// caller's (`what_depends_on`) return shape. Gated so a lean build — which has
/// no agent surface to serve it to — doesn't carry a dead type. A11Y3 adds
/// `dev-observability` to the gate: its only producer, `dependents_of`, scans
/// the per-node dep keys that feature collects.
#[cfg(all(feature = "snapshot", feature = "dev-observability"))]
struct DepEntry {
    /// Node index (serialized as `node-<index>`).
    node: u32,
    /// Which prop the dependency is through: `scope` / `text` / `background`.
    via: &'static str,
    /// How a change propagates: `rebuild` (scope/text) or `patch` (background).
    update: &'static str,
}

/// Per-rebuild style-resolution environment (A.2): the cascade sources and
/// token table, computed once per rebuild and consumed inline by
/// `build_node` so `.lss` layout properties reach taffy *before* layout.
/// B.2: also carries the live [`lumen_style::MediaContext`] so `@media`
/// blocks gate on the real window instead of applying unconditionally.
struct StyleEnv {
    sources: [lumen_style::StyleSource; 1],
    tokens: lumen_style::Tokens,
    media: lumen_style::MediaContext,
    /// B.5b: `@keyframes` timelines, each stop pre-applied into the paint
    /// tier once per rebuild (name → sorted `(pct, values)` stops).
    keyframes: HashMap<String, Vec<(f32, KeyStop)>>,
}

/// One evaluated `@keyframes` stop's paint-tier values (B.5b).
#[derive(Clone, Copy, Default)]
struct KeyStop {
    background: Option<Color>,
    color: Option<Color>,
    opacity: Option<f32>,
    border_radius: Option<f32>,
}

/// A `cx.scope`'s recorded node span (A.3.1) plus the soundness context for
/// copy-forward (A.3.2): the hash covers everything *outside* the scope that
/// its per-node work depended on (ancestor selector chain, container size,
/// overlay/hidden state); `impure` marks spans whose lowering has per-node
/// side work (dyn bindings, custom measure) that must re-run every build.
#[derive(Clone, Copy)]
struct SpanRec {
    root: NodeIndex,
    count: u32,
    /// F2.4: 128-bit. This value decides whether a span is spliced, so a
    /// collision shows up as a stale subtree on screen, not as a slow frame.
    ctx_hash: IdHash,
    impure: bool,
}

/// The rare half of [`NodeMeta`] — see its `rare` field.
#[derive(Default)]
pub(crate) struct RareMeta {
    on_wheel: Option<crate::element::WheelHandler>,
    on_drag: Option<crate::element::DragHandler>,
    on_drop: Option<crate::element::DropHandler>,
    on_text: Option<crate::element::TextHandler>,
    on_key: Option<crate::element::KeyHandler>,
    on_caret_set: Option<crate::element::CaretHandler>,
    on_dismiss: Option<Handler>,
    on_increment: Option<Handler>,
    on_decrement: Option<Handler>,
    on_set_value: Option<crate::element::ValueHandler>,
    caret_byte: Option<usize>,
    selection: Option<(usize, usize)>,
    scroll: Option<lumen_core::semantics::ScrollInfo>,
    shadow: Option<crate::element::Shadow>,
    set_size: Option<usize>,
    position_in_set: Option<usize>,
}

impl NodeMeta {
    fn on_wheel(&self) -> Option<&crate::element::WheelHandler> {
        self.rare.as_ref().and_then(|r| r.on_wheel.as_ref())
    }
    fn on_drag(&self) -> Option<&crate::element::DragHandler> {
        self.rare.as_ref().and_then(|r| r.on_drag.as_ref())
    }
    fn on_drop(&self) -> Option<&crate::element::DropHandler> {
        self.rare.as_ref().and_then(|r| r.on_drop.as_ref())
    }
    fn on_text(&self) -> Option<&crate::element::TextHandler> {
        self.rare.as_ref().and_then(|r| r.on_text.as_ref())
    }
    fn on_key(&self) -> Option<&crate::element::KeyHandler> {
        self.rare.as_ref().and_then(|r| r.on_key.as_ref())
    }
    fn on_caret_set(&self) -> Option<&crate::element::CaretHandler> {
        self.rare.as_ref().and_then(|r| r.on_caret_set.as_ref())
    }
    fn on_dismiss(&self) -> Option<&Handler> {
        self.rare.as_ref().and_then(|r| r.on_dismiss.as_ref())
    }
    fn on_increment(&self) -> Option<&Handler> {
        self.rare.as_ref().and_then(|r| r.on_increment.as_ref())
    }
    fn on_decrement(&self) -> Option<&Handler> {
        self.rare.as_ref().and_then(|r| r.on_decrement.as_ref())
    }
    fn on_set_value(&self) -> Option<&crate::element::ValueHandler> {
        self.rare.as_ref().and_then(|r| r.on_set_value.as_ref())
    }
    fn scroll(&self) -> Option<&lumen_core::semantics::ScrollInfo> {
        self.rare.as_ref().and_then(|r| r.scroll.as_ref())
    }
    fn shadow(&self) -> Option<&crate::element::Shadow> {
        self.rare.as_ref().and_then(|r| r.shadow.as_ref())
    }
    fn caret_byte(&self) -> Option<usize> {
        self.rare.as_ref().and_then(|r| r.caret_byte)
    }
    fn selection(&self) -> Option<(usize, usize)> {
        self.rare.as_ref().and_then(|r| r.selection)
    }
}

pub(crate) struct NodeMeta {
    id: Option<StableId>,
    role: Role,
    label: String,
    value: Option<String>,
    classes: Vec<String>,
    actions: Vec<Action>,
    states: Vec<SemState>,
    focusable: bool,
    autofocus: bool,
    elide: bool,
    /// Per-prop signal dependencies (F2 union → semantics; F4 breakdown).
    /// A11Y3: agent-only. `ui.getDeps` and the reverse index are the sole
    /// readers — reactivity itself runs off `BoundText`/`BoundBg`'s `Reads`,
    /// not this. 96 bytes (4 × `Vec<String>`) on a struct that is built for
    /// every node of every frame, so unlike the `SemanticsNode` payload this
    /// one sits on the hot path.
    #[cfg(feature = "dev-observability")]
    deps: NodeDeps,
    on_click: Option<Handler>,
    background: Option<Color>,
    border: Option<Border>,
    corner_radius: f64,
    clip: bool,
    overlay: bool,
    /// Rust-side pointer shape (a `.lss` `cursor` rule overrides it).
    cursor: Option<lumen_core::CursorShape>,
    /// Typed inline style (B.6b) — retained so the A.5 restyle path can
    /// re-merge it after re-resolving sheet rules.
    css_inline: Option<Box<lumen_style::Style>>,
    content: NodeContent,
    /// Left/top padding in px — own-text is painted at the padded (content-box)
    /// origin, so a button label sits inside its padding instead of jammed into
    /// the border-box corner.
    pad: (f64, f64),
    /// Content-box wrap width in px for a wrapping text paragraph (set when the
    /// element carries an explicit pixel width). `None` = size-to-content, no
    /// wrap. The paint pass must lay out with the same width as the measure pass.
    wrap_width: Option<f32>,
    /// PROP1 `text-overflow: ellipsis`: the truncated string the PAINT pass
    /// draws. The node's own text (and therefore the semantic tree, the agent
    /// and assistive tech) keeps the FULL string — truncating that would make
    /// `ui.getTree` report "Some long lab…", corrupting the observability
    /// surface to fix a visual one.
    display_text: Option<String>,
    /// O0.13: the fields almost no node has — every event handler past
    /// `on_click`, the caret/selection pair, scroll state and the shadow.
    ///
    /// Inline they were **304 of `NodeMeta`'s 816 bytes**, present as `None`
    /// on every label in every list. `meta` is not only written once per node
    /// per rebuild but *walked* several times a frame by the audit, so the
    /// bytes are paid on both. Boxed, the common node carries one null
    /// pointer and the map's working set shrinks by more than a third.
    rare: Option<Box<RareMeta>>,
}

/// The px value of a [`Dim`] (0 for non-px / auto / percent).
fn dim_px(d: Dim) -> f64 {
    match d {
        Dim::Px(v) => v as f64,
        _ => 0.0,
    }
}

/// A headless, CPU-rendered application instance (02 §8). Drives the same input
/// queue as a real shell, so tests and the agent exercise the real paths.
pub struct Headless<
    R = lumen_render::DefaultRenderer,
    E = lumen_core::tasks::InlineSpawner,
    P: PlatformConfig = DefaultPlatform,
> {
    /// The root view.
    ///
    /// Returns a boxed `DirectDyn` rather than an `Element`: the *root* is the
    /// one place a box is unavoidable, because the closure's return type is
    /// erased by storage, and it is one box per frame rather than one per node.
    root: RootView,
    /// P.3d: declared secondary windows (descriptor + root closure), realized
    /// on demand by [`open_window`](Self::open_window). `windows` (below)
    /// carries just the descriptors for the agent.
    window_decls: Vec<WindowDecl>,
    /// App-registered font bytes, retained so secondary windows register the
    /// same faces into their own `TextEngine` (one copy per opened window —
    /// windows are rare; the main engine's zero-copy registration stands).
    font_bytes: Vec<Vec<u8>>,
    /// Raw `.lss` source, retained so secondary windows boot with the same
    /// stylesheet (each window cascades independently at its own size).
    stylesheet_src: Option<String>,
    rt: Runtime,
    /// Logical size (the coordinate space for layout, events, and the display
    /// list). The rasterized frame is this times [`Headless::scale`].
    size: Size,
    /// HiDPI scale factor: the frame is rendered at `size * scale` physical px.
    scale: f64,
    clock_ms: f64,
    /// The frame renderer backend `R` (A1). The runtime is generic over it,
    /// chosen at construction (`App::with_renderer`); defaults to the CPU
    /// reference renderer. Zero-cost by default; `R = Box<dyn Renderer>` opts
    /// into dynamic dispatch.
    renderer: R,
    /// The background-work executor `E` (the data layer). Generic, chosen at
    /// construction (`App::with_executor`); defaults to the deterministic inline
    /// executor. `E = Box<dyn Spawner>` opts into dynamic dispatch.
    executor: E,
    /// Host waker: wakes the event loop when a background result is queued, so a
    /// frame gets scheduled. Set by the shell; `None` headless (the next manual
    /// `pump` drains the deferred-op queue).
    task_waker: Option<lumen_core::tasks::WakeFn>,
    text: P::Text,
    /// Cache of rasterized text keyed by (string, size bits, weight bits, sRGB
    /// color): static labels then cost one memcpy per frame instead of a full
    /// reshape + glyph raster. Cleared wholesale when it exceeds a cap so an
    /// animated readout (many distinct strings) can't grow it without bound.
    text_cache: HashMap<(String, u32, u32, u32, u32), RgbaImage>,
    /// Cache of rasterized drop shadows keyed by quantized (w, h, radius, blur,
    /// spread, color). The stacked-rounded-rect penumbra is the single most
    /// expensive thing in a typical frame; since it's static for a given box it
    /// is rendered once and then blitted as one image.
    shadow_cache: HashMap<(i32, i32, i32, i32, i32, u32), RgbaImage>,
    tree: Tree,
    meta: HashMap<NodeIndex, NodeMeta>,
    /// Rendered *ink* bounds per node from the last paint — the union of what a
    /// node actually painted (text uses the glyph-ink `run_rect`, which can extend
    /// past the layout box via descenders/side bearings). Absent ⇒ ink == box.
    /// Drives the clipping audit (W0104) and `ui.getLayout`'s `ink`.
    /// A11Y3: collected only under `dev-observability`. Nothing outside the
    /// agent and the (equally gated) W0104 clipping audit reads it, so a
    /// shipped build has no reason to carry it. Measured: gating the *writes*
    /// changes frame time by nothing at all (paint culls, so ~20 text nodes
    /// reach the insert), which is why this is a footprint change and not a
    /// speed one — see the task-graph entry.
    #[cfg(feature = "dev-observability")]
    node_ink: HashMap<NodeIndex, kurbo::Rect>,
    /// The painted caret rectangle (window-space) per focused editor,
    /// repopulated each display-list pass. Introspection for [`Headless::caret_rect`].
    node_caret: HashMap<NodeIndex, kurbo::Rect>,
    /// Typographic metrics per text node from the last paint (diagnostic aid;
    /// surfaced on `SemanticsNode.text_metrics` and via `ui.getLayout`).
    #[cfg(feature = "dev-observability")]
    node_text_metrics: HashMap<NodeIndex, lumen_text::TextMetrics>,
    frame: RgbaImage,
    /// OB2: the semantics tree, built **on demand**.
    ///
    /// It used to be rebuilt eagerly in every `rebuild_inner`, which measured at
    /// **8.9% of a 1000-row frame** — paid whether or not anything ever read it.
    /// An app with no screen reader attached and no agent connected never does.
    /// A rebuild now invalidates this; the first reader builds it and the rest
    /// share the `Rc`.
    sem_root: RefCell<Option<Rc<SemanticsNode>>>,
    /// O0.4: node index → handle, built once per semantic tree.
    ///
    /// `handle_for_index` used to walk the whole semantic tree per call, and
    /// every lint finding calls it once to attach a target handle. That is
    /// O(nodes) per finding, so a frame with many findings is O(nodes^2) — and
    /// findings are not rare: a column laid out taller than the window makes
    /// **every** row an offscreen finding, which is the normal shape of a long
    /// page. Measured on a 6600-node view: 35 ms in `offscreen_findings`
    /// alone, 82% of the frame, on a frame whose memo was hitting perfectly.
    ///
    /// Derived from `sem_root`, so it is invalidated with it.
    handle_index: RefCell<Option<Rc<HashMap<u32, lumen_core::identity::NodeHandle>>>>,
    /// If the last build panicked, the contained diagnostic (the previous good
    /// frame is kept). Cleared on the next successful build (C2 / T7.3).
    build_panic: Option<lumen_core::Diagnostic>,
    focused_id: Option<StableId>,
    hovered_id: Option<StableId>,
    /// The node being dragged: its index *and* stable id (if any). The id lets a
    /// drag survive rebuilds that renumber nodes (e.g. a scrollbar whose index
    /// shifts as list rows load) by re-resolving the current node each move.
    pressed: Option<(NodeIndex, Option<StableId>)>,
    /// A press that may still become a click: the `on_click` node it landed on
    /// (index *and* stable id, re-resolved on release like [`Self::pressed`]),
    /// and the press position the movement slop is measured from.
    ///
    /// `on_click` fires on **release**, not press, so a finger that presses a
    /// row and then drags to scroll does not activate it. Cleared by a touch
    /// that travels past [`TOUCH_SLOP_PX`], by a release that lands somewhere
    /// else, and by the next press.
    pending_click: Option<(NodeIndex, Option<StableId>, Point)>,
    /// Touch panning: the node whose wheel handler a finger drag drives, plus
    /// the last pointer position. Armed on a touch press that did not land on a
    /// drag handler; cleared on release.
    ///
    /// TOUCH ONLY, deliberately: a mouse drag inside a list is a text selection
    /// or a marquee, not a scroll.
    pan: Option<(NodeIndex, Point)>,
    /// Velocity estimate for the active pan, in px/s, plus the clock time it was
    /// sampled at. Feeds [`Headless::fling`] on release.
    pan_vel: (f64, f64, f64),
    /// Momentum after a finger lifts: `(pos, vx, vy)` in px/s. Each frame emits a
    /// decaying delta through the same wheel dispatcher, so a coast obeys scroll
    /// chaining and clamping exactly like a drag does.
    fling: Option<(Point, f64, f64)>,
    /// Clock time the fling was last stepped, so decay tracks elapsed time
    /// rather than frame count.
    fling_ms: f64,
    app_sheet: Option<lumen_style::Stylesheet>,
    theme: lumen_style::ThemeKind,
    node_style: HashMap<NodeIndex, Styled>,
    node_computed: HashMap<NodeIndex, Computeds>,
    /// A.2: per-rebuild cascade env (None when no stylesheet is attached).
    style_env: Option<StyleEnv>,
    /// A.3.1: per-rebuild scope→node-span map (scope key → subtree root +
    /// preorder node count). The retained-graph splice replaces these spans.
    scope_spans: HashMap<IdHash, SpanRec>,
    /// Last build's span records (A.3.2 / F2.2). A memo-hit scope whose
    /// recorded context hash matches has its span spliced into this build
    /// instead of being re-lowered.
    ///
    /// F2.2 deleted this field's five companions — `prev_tree`, `prev_meta`,
    /// `prev_node_style`, `prev_node_computed`, `prev_layout_style` — along
    /// with the `prev_spans_by_root` index CP1 added to make the nested-span
    /// remap O(1) per node. With the arena retained, none of that work exists:
    /// the previous build's tree *is* the current tree, its per-node entries
    /// are already keyed by the indices they still have, and nested spans
    /// still name the same roots, so there is nothing to remap.
    prev_spans: HashMap<IdHash, SpanRec>,
    /// Final (post-css-merge) layout style per node — retained across frames
    /// so a spliced span never re-derives it from the element.
    /// Whether this rebuild may copy spans forward (false on visual-state
    /// rebuilds: hover/focus/pressed styling must re-resolve).
    /// R6: the layout engine, retained across frames as scratch.
    ///
    /// It is genuinely per-frame — the solved bounds are copied into the node
    /// arena and the tree discarded — but recreating it meant allocating and
    /// freeing the whole slotmap every frame. Kept and `clear`ed instead, so
    /// the capacity survives.
    layout_scratch: P::Layout,
    /// F2.1: whether the layout engine keeps its nodes across frames, so a
    /// memo-hit span can reuse them instead of re-creating them. Read once per
    /// rebuild from `LayoutEngine::retains_nodes` and cached here because
    /// `copy_node` consults it per node.
    layout_reuse: bool,
    allow_copy_forward: bool,
    /// Count of elements this build encountered carrying non-memoizable
    /// per-node work (dyn bindings, custom/canvas content) — spans containing
    /// any are never copied forward.
    impure_seen: u32,
    nodes_rebuilt: u32,
    nodes_copied: u32,
    /// O1.3: lifetime totals, distinct from the per-pump counters above.
    ///
    /// The per-pump values are reset at the top of EVERY pump, and the
    /// recommended agent sequence (interact → `ui.waitSettled` → `app.perf`)
    /// necessarily ends on idle pumps — `waitSettled` loops until the UI is
    /// quiescent — so by the time `app.perf` is read they are always 0/0.
    /// Cumulative counters are readable by bracketing an interaction with two
    /// reads and subtracting, which is the pattern `style_memo_hits`/`misses`
    /// already forced; now the whole response is consistent about it.
    nodes_rebuilt_total: u64,
    nodes_copied_total: u64,
    /// A.5b: memoized style resolution — (node desc + ancestor context) →
    /// resolved pair. Most nodes share a handful of keys, so a rebuild does
    /// O(distinct keys) cascades instead of O(nodes). Cleared with the view
    /// caches (stylesheet/theme/resize force-rebuilds).
    style_memo: HashMap<IdHash, StylePair>,
    /// W0404: text nodes this build had to shape at layout time because a
    /// content-sizing container needed their intrinsic width.
    shaped_for_indefinite: usize,
    style_memo_hits: u64,
    style_memo_misses: u64,
    /// B.5: running `transition:` animations keyed by (stable id, property).
    /// Identity is id-based — transitions only animate on nodes with stable
    /// ids (others snap); GC'd when the id leaves the tree.
    prop_anims: HashMap<(StableId, &'static str), PropAnim>,
    /// B.5: OS reduced-motion (04 §3) — durations clamp to 0.
    reduced_motion: bool,
    /// B.5b: running `animation:` timelines — id → (start_ms, finished).
    key_anims: HashMap<StableId, (f64, bool)>,
    /// B.5b: theme-switch animation window — until this clock time, color
    /// properties get an implicit 150 ms transition (04 §4).
    theme_anim_until: f64,
    /// C.4b: named app commands from the last build
    /// (`cx.register_command`) — `run_command` invokes by name.
    commands: HashMap<String, crate::element::Handler>,
    /// B.1: the ancestor descriptors of the element currently being lowered
    /// (root-first), fed to `resolve_with_ancestors` so descendant/`>`
    /// selectors match correctly. Maintained by `build_node`'s recursion.
    desc_stack: Vec<std::rc::Rc<lumen_style::NodeDesc>>,
    /// O0.8: `desc_hash_stack[i]`, once computed, is an [`IdHasher`] that has
    /// absorbed `desc_stack[0..i]` — the ancestor-chain prefix of
    /// `span_ctx_hash`, memoized per depth so siblings share it.
    ///
    /// Filled **lazily**, which is the whole point. A leaf is pushed onto
    /// `desc_stack` like any other node, but nothing ever asks for the prefix
    /// *below* a leaf, because only a child would — so a flat list of 2000
    /// rows hashes its one shared ancestor once instead of hashing something
    /// 2000 times. Eagerly absorbing on push measured *worse* than the walk it
    /// replaced, for exactly that reason.
    ///
    /// Length is always `desc_stack.len() + 1`; the two are only mutated
    /// together, through `push_desc`/`pop_desc`.
    desc_hash_stack: std::cell::RefCell<Vec<Option<lumen_core::identity::IdHasher>>>,
    /// B.2b: container-query support. `container_nodes` — the `.container()`
    /// nodes of the current tree in build order; `container_prev` — their
    /// sizes from the last layout (what the build resolves against);
    /// `container_stack` — build-time stack of the nearest enclosing
    /// container's size (`None` = not yet measured); `container_repass` —
    /// re-entrancy guard for the bounded post-layout re-pass.
    container_nodes: Vec<NodeIndex>,
    container_prev: Vec<(f64, f64)>,
    container_stack: Vec<Option<(f64, f64)>>,
    container_repass: bool,
    /// B.3 `visibility:hidden` — depth of enclosing hidden subtrees during
    /// build; > 0 clears VISIBLE/HIT_TESTABLE on every node built inside.
    hidden_count: usize,
    /// W1 `disabled` — depth of enclosing disabled subtrees during build, so a
    /// child of a disabled container also matches `:disabled` in `.lss` (the
    /// enforcement half is `propagate_disabled` over the finished tree).
    disabled_count: usize,
    /// C.2: rolling per-painted-frame pump durations in ms (cap 120) + total
    /// painted frames — the agent's `app.perf`. Diagnostic only (never feeds
    /// rendering); not recorded on wasm (no `Instant`).
    /// O0.3: previous painted frame's lint-finding keys, for the ambient audit.
    /// Present only where the audit is compiled in.
    #[cfg(feature = "dev-observability")]
    audit_diff: lumen_core::observe::FrameDiff<String>,
    /// O0.3: bumped whenever the semantic tree is replaced; compared against
    /// `last_audit_gen` to decide whether the ambient audit has work to do.
    /// O0.15: `last_audit_ms` throttles how often it may act on that.
    #[cfg(feature = "dev-observability")]
    sem_gen: std::cell::Cell<u64>,
    #[cfg(feature = "dev-observability")]
    last_audit_gen: u64,
    #[cfg(feature = "dev-observability")]
    last_audit_ms: f64,
    /// O1.2: the last damage that actually painted, with its frame number.
    /// `last_damage` is per-pump and an idle frame clears it.
    last_paint_damage: (Damage, u64),
    frame_ms: std::collections::VecDeque<f32>,
    /// O1.3: the worst painted frame since start, and how many blew the
    /// budget. A rolling p95 of 6 ms with one 300 ms stall reads healthy and
    /// feels broken — the percentiles are computed over a 120-frame window, so
    /// a single stall is gone from them within two seconds of scrolling.
    frame_ms_max: f32,
    frames_over_budget: u64,
    /// C.2: total painted frames since launch.
    frames_rendered: u64,
    input: InputQueue,
    pointer: PointerState,
    // Animation/timer requests from the latest build (02 §8, time-driven UI).
    requests: crate::element::FrameRequests,
    // Desktop system integration (T5.2). The clipboard lives on the Runtime so
    // event handlers can reach it; see `Runtime::clipboard`.
    menu: crate::system::MenuModel,
    menu_rev: u64,
    invoked_menu: Vec<String>,
    system_requests: Vec<crate::system::SystemRequest>,
    windows: Vec<crate::system::WindowDesc>,
    rtl: bool,
    /// Previous frame's display list, retained so the next paint can compute a
    /// damage region and repaint only what changed (R2). `None` forces a full
    /// repaint (first frame, or after a resize/scale change).
    last_dl: Option<DisplayList>,
    /// Damage applied by the most recent paint (reported via [`FrameStats`]).
    last_damage: lumen_render::Damage,
    /// True once a live window surface is wired to the renderer (1c). The build
    /// then presents straight to the swapchain instead of rasterizing to
    /// `self.frame`; `screenshot()` renders on demand. Always false headless.
    surface_attached: bool,
    /// `Runtime::write_gen()` captured after the last rebuild — `pump` compares it
    /// to the current value to detect whether any signal changed, and skips the
    /// rebuild entirely when nothing did (idle/non-effecting frames cost ~µs).
    last_build_gen: u64,
    /// Forces the next `pump` to rebuild regardless of reactive state — set by
    /// resize/scale/stylesheet/theme changes and `force_full_repaint`, which alter
    /// the frame without going through a signal.
    force_rebuild: bool,
    /// `clock_ms` at the last rebuild. If the last build read the clock
    /// (`requests.read_clock`), `pump` rebuilds whenever the clock has advanced
    /// past this — so time-driven UI updates even without an explicit `animate`/
    /// `wake_at`.
    last_build_clock: f64,
    /// Memoized `cx.scope` subtrees (F1), persisted across builds. A rebuild
    /// reuses a scope's cached subtree while none of the signals it read has
    /// changed; cleared by `clear_view_caches` (the oracle + non-signal
    /// rebuilds). Coherence is guarded by `assert_view_coherent` (F0).
    scope_cache: RefCell<crate::element::ScopeCache>,
    /// Scope keys accessed during the current build (F5 GC). After the build,
    /// cached scopes + scope-local signals whose key is absent are swept.
    scope_live: RefCell<crate::fxhash::HashSet<IdHash>>,
    /// Scopes that memo-hit during the current build. Their children never got
    /// to announce themselves in `scope_live`, so the sweep treats a scope with
    /// a skipped ancestor as live (F5 × F1).
    scope_skipped: RefCell<crate::fxhash::HashSet<IdHash>>,
    /// Live background tasks by identity (TC1). Declaring a task registers its
    /// slot; `sweep_dead_scopes` cancels the ones whose owning scope vanished,
    /// and dropping the table on teardown cancels the rest.
    tasks_table: RefCell<crate::element::TaskTable>,
    /// Retained paint-only prop bindings from the last build (F3.4). A change to
    /// one binding's deps patches its node + repaints, skipping the rebuild.
    bg_bindings: Vec<BoundBg>,
    /// MUT1: SignalId → binding slots. Rebuilt after every rebuild and kept
    /// current across patches whose read set changed, so the pump resolves a
    /// write to its bindings in O(writes) instead of scanning every binding.
    binding_index: HashMap<SignalId, Vec<BindingSlot>>,
    /// MUT2: bound nodes' display-list footprints, refreshed by every
    /// `build_display_list`. Lets a patch frame rewrite exactly the changed
    /// commands instead of rebuilding and diffing the whole list.
    dl_patch: HashMap<NodeIndex, DlSlot>,
    /// F3.5: retained text bindings — see [`BoundText`].
    text_bindings: Vec<BoundText>,
    /// Signals whose change requires a structural rebuild (root + scope + text-
    /// binding reads; paint-only bindings are isolated out). `is_current` false ⇒
    /// rebuild; else a paint-only binding change can be patched (F3.4).
    structural_reads: lumen_core::state::ReadSet,
    /// OB4: memoized `sem_root.elided()`.
    ///
    /// `semantics_doc()` deep-clones the whole tree, and almost every caller
    /// immediately calls `.root.elided()`, which clones the surviving subtree
    /// again — twice per agent RPC, per assertion, per a11y publish. The elided
    /// projection is a pure function of `sem_root`, so it is computed once and
    /// shared as an `Rc`; `invalidate_semantics_cache` clears it wherever
    /// `sem_root` is reassigned.
    elided_cache: RefCell<Option<Rc<lumen_core::semantics::SemanticsNode>>>,
    /// Memoized `semantics_doc().to_json(raw)`, indexed by `raw as usize`.
    ///
    /// Serializing the tree measured **1106 µs at 1000 nodes** — 18× the cost of
    /// building it — and it was recomputed on every call, so an agent polling
    /// `ui.getTree` paid a millisecond each time. The JSON is a pure function of
    /// `sem_root` + window info, so it is computed once per rebuild and shared;
    /// `invalidate_semantics_cache` clears it alongside the elided projection.
    #[cfg(feature = "snapshot")]
    json_cache: RefCell<[Option<Rc<serde_json::Value>>; 2]>,
    /// What the last `pump` actually did (F4.3 change attribution).
    #[cfg(feature = "snapshot")]
    last_change: ChangeReport,
}

/// One animated property value (B.5): color or scalar.
#[derive(Clone, Copy, PartialEq, Debug)]
enum AnimVal {
    Color(Color),
    Num(f32),
}

impl AnimVal {
    fn blend(a: AnimVal, b: AnimVal, t: f32) -> AnimVal {
        match (a, b) {
            (AnimVal::Color(x), AnimVal::Color(y)) => AnimVal::Color(x.lerp_oklab(y, t)),
            (AnimVal::Num(x), AnimVal::Num(y)) => AnimVal::Num(x + (y - x) * t),
            (_, b) => b,
        }
    }
}

/// Sample a keyframe timeline at `phase` for one property (B.5b): find the
/// bracketing stops that define it and blend with the segment-local eased t.
fn sample_stops(
    stops: &[(f32, KeyStop)],
    phase: f32,
    easing: lumen_style::Easing,
    get: impl Fn(&KeyStop) -> Option<AnimVal>,
) -> Option<AnimVal> {
    let defined: Vec<(f32, AnimVal)> = stops
        .iter()
        .filter_map(|(p, st)| get(st).map(|v| (*p, v)))
        .collect();
    if defined.is_empty() {
        return None;
    }
    if phase <= defined[0].0 {
        return Some(defined[0].1);
    }
    for w in defined.windows(2) {
        let (p0, v0) = w[0];
        let (p1, v1) = w[1];
        if phase <= p1 {
            let t = if p1 > p0 {
                (phase - p0) / (p1 - p0)
            } else {
                1.0
            };
            return Some(AnimVal::blend(v0, v1, easing.apply(t)));
        }
    }
    Some(defined.last().unwrap().1)
}

/// A running `transition:` for one (node id, property) pair (B.5).
#[derive(Clone, Debug)]
struct PropAnim {
    from: AnimVal,
    to: AnimVal,
    start_ms: f64,
    duration_ms: f32,
    delay_ms: f32,
    easing: lumen_style::Easing,
    /// The final value has been resolved into a build. An anim stays
    /// "active" until then, so the pump schedules the one last rebuild
    /// that lands exactly on the target.
    committed: bool,
}

impl PropAnim {
    fn progress(&self, now: f64) -> f32 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }
        (((now - self.start_ms) as f32 - self.delay_ms) / self.duration_ms).clamp(0.0, 1.0)
    }
    fn value_at(&self, now: f64) -> AnimVal {
        let t = self.progress(now);
        AnimVal::blend(self.from, self.to, self.easing.apply(t))
    }
    fn done(&self, now: f64) -> bool {
        self.progress(now) >= 1.0
    }
}

/// The pre-routing snapshot of input-driven visual state
/// (hover/focus/pressed) — what [`Headless::restyle_visual`] diffs against.
type VisualState = (
    Option<StableId>,
    Option<StableId>,
    Option<(NodeIndex, Option<StableId>)>,
);

/// The computed-value map (`get_styles` form), SHARED rather than copied.
///
/// O0.6: this map is the observability half of the cascade — its only reader
/// is `get_styles`, the agent introspection call — and it was deep-cloned out
/// of the A.5b memo for every node of every rebuild. Because the memo key is
/// the node's *whole* style identity, every node that resolves alike wants the
/// same map, so the copies were byte-identical: a 2000-row styled frame paid
/// ~172 us re-allocating one String key per declaration per node for a map
/// that is only ever read on demand. `Rc` makes the common path a refcount
/// bump; the two writers that really do mutate (an inline style, a restyle)
/// take a private copy through `Rc::make_mut`.
type Computeds = std::rc::Rc<HashMap<String, lumen_style::Computed>>;

/// The resolved typed style, SHARED rather than copied.
///
/// O0.10: `Style` is **1008 bytes**, and it was cloned out of the A.5b memo
/// for every node and then moved into `node_style` — 4 MB of memcpy each way
/// on a 4000-node frame, for a value that is byte-identical across every node
/// resolving alike (the memo key is the node's whole style identity). The only
/// writers after resolution are an inline style, a transition and a keyframe,
/// each of which announces itself in the style it is about to modify, so the
/// fork can be gated on "will anything actually write".
type Styled = std::rc::Rc<lumen_style::Style>;

/// What the A.5b resolution memo caches: the node descriptor, the typed style,
/// and the computed-value map. All three are `Rc`, so a hit is three refcount
/// bumps and no allocation at all — O0.11 folded the descriptor in because it
/// is a pure function of the same identity the key already collapses.
type StylePair = (std::rc::Rc<lumen_style::NodeDesc>, Styled, Computeds);

/// A node's re-resolved style pair, pending commit (A.5 two-pass restyle).
type PendingStyle = (NodeIndex, Styled, Computeds);

/// What a `pump` did, for change attribution (F4.3).
#[derive(Clone, Default)]
/// What the last pump did (F4.3), for `ui.lastChange`.
///
/// Snapshot-only. The four write sites route through `record_change`, which
/// compiles away in a lean build — previously they ran unconditionally while
/// the only reader was gated, so a lean build allocated a `Vec<u32>` per pump
/// for a value nothing could observe.
#[cfg(feature = "snapshot")]
struct ChangeReport {
    /// `"idle"`, `"patch"` (paint-only bindings), or `"rebuild"` (structural).
    kind: &'static str,
    /// Node indices that were patched/rebuilt-with-changed-output this pump.
    nodes: Vec<u32>,
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig>
    Headless<R, E, P>
{
    /// Process the input queue, then rebuild/layout/paint/semantics one turn —
    /// unless nothing that affects the frame changed, in which case the rebuild is
    /// skipped entirely (idle/non-effecting frames cost ~µs instead of ~ms).
    pub fn pump(&mut self) -> FrameStats {
        // C.2: time painted pumps for `app.perf`. Diagnostic-only wall time —
        // it never feeds rendering, so the pure-function contract holds.
        #[cfg(not(target_arch = "wasm32"))]
        let pump_t0 = std::time::Instant::now();
        // O0.15: the semantics generation on entry, so the ambient audit can
        // tell "this pump moved the view" from "the view is at rest".
        #[cfg(feature = "dev-observability")]
        let gen_on_entry = self.sem_gen.get();
        // A.3.2 meters reflect *this* pump: idle/patch-only pumps report 0.
        self.nodes_rebuilt = 0;
        self.nodes_copied = 0;
        // Apply any background-task results first (on the UI thread), so the build
        // sees fresh state. Keeps `pump` a pure function of (state, queued
        // events + deferred ops, clock). Deferred results write signals → bump the
        // reactive write-gen, which the skip check below observes.
        // O4.5: `drain_deferred` returns a count that was discarded. "Your data
        // arrived on frame N" is the line that separates "the fetch never
        // completed" from "it completed and the view ignored it" — two very
        // different bugs that look identical from outside.
        #[cfg(feature = "dev-observability")]
        {
            let applied = self.rt.drain_deferred();
            if applied > 0 {
                self.rt.log(
                    "info",
                    format!("{applied} background result(s) applied on this frame"),
                );
            }
        }
        #[cfg(not(feature = "dev-observability"))]
        self.rt.drain_deferred();
        // Input-driven visual state that doesn't go through a signal (hover/focus/
        // pressed). Snapshot it to detect changes from routing.
        let visual_before = (
            self.hovered_id.clone(),
            self.focused_id.clone(),
            self.pressed.clone(),
        );
        let mut events = Vec::new();
        while let Some(ev) = self.input.pop() {
            events.push(ev);
        }
        for ev in events {
            self.route(ev);
        }
        // AFTER routing, so a touch arriving in this same batch cancels the
        // coast before it advances another frame — pressing a coasting list
        // stops it on the frame you touch it, not the one after.
        self.step_fling();
        // W.2: handler-enqueued SystemRequests ride the runtime's host
        // mailbox; drain after routing so a click's request is visible to
        // the host/agent in the same pump.
        self.system_requests
            .extend(self.rt.take_posted::<crate::system::SystemRequest>());
        // Rebuild only when something that affects the frame changed:
        //  - a signal/memo write since the last build (reactive write-gen),
        //  - input-driven visual state (hover/focus/pressed) changed,
        //  - the UI is time-driven this tick (continuous, or a one-shot wake came
        //    due since the last build),
        //  - or a forced invalidation (resize/scale/stylesheet/theme/repaint).
        // Conservative: anything uncertain forces a rebuild (set bumps the write-
        // gen even on equal values; any visual delta counts).
        let visual_changed = (
            self.hovered_id.clone(),
            self.focused_id.clone(),
            self.pressed.clone(),
        ) != visual_before;
        // Time-driven iff the last build read the clock (or asked to animate) AND
        // the clock has advanced since that build — then the frame would differ.
        let anims_running = self.anims_active();
        let time_driven = (self.requests.read_clock || self.requests.continuous || anims_running)
            && self.clock_ms != self.last_build_clock;
        let write_changed = self.rt.write_gen() != self.last_build_gen;
        // MUT1: drain this pump's written signals unconditionally so the log
        // cannot grow across pumps; entries a rebuild makes moot filter out as
        // current on the next resolution.
        let written = self.rt.take_written();
        // F3.4: a structural signal changed ⇒ rebuild; a change confined to
        // paint-only (background) bindings ⇒ patch that node + repaint, no
        // rebuild/relayout. `structural_reads` is every build-time read except
        // isolated paint-only bindings.
        let structural_current = self.structural_reads.is_current(&self.rt);
        let needs_rebuild =
            self.force_rebuild || time_driven || (write_changed && !structural_current);
        // A.5: a pump where ONLY visual state (hover/focus/pressed) changed
        // takes the restyle-only path — flags + affected subtree styles +
        // repaint, no closure runs, no lowering, no relayout. Falls back to a
        // full rebuild when a state rule touches layout/typography (the A.2
        // risk note) so layout stays correct.
        let restyle_only = visual_changed && !needs_rebuild && !full_rebuild_forced();
        // MUT1: written signals → stale binding indices via the reverse index.
        let (stale_text, stale_bg) = if needs_rebuild {
            (Vec::new(), Vec::new())
        } else {
            self.stale_bindings(&written)
        };
        // The shape/run caches evict by frame epoch: entries used this frame or
        // last are the live working set and must survive a cap crossing. Advance
        // the epoch only on frames that actually shape — an idle pump shapes
        // nothing, and advancing through an idle stretch would age the whole
        // live set out and hand the next sweep a full re-shape stall.
        if needs_rebuild || restyle_only {
            self.text.begin_frame();
        }
        if needs_rebuild || (restyle_only && !self.restyle_visual(&visual_before)) {
            // Scope memoization keys off signal versions only, so a rebuild
            // driven by a forced invalidation (resize/scale/stylesheet/theme —
            // inputs a build can observe through `cx`) must not reuse stale
            // subtrees: drop the caches and let this build repopulate.
            //
            // Visual state (hover/focus/pressed) deliberately does NOT clear
            // them (A.1, docs/plan-retained-pipeline.md): `BuildCx` exposes no
            // accessor for it, so no view closure can depend on it — it is
            // applied *after* the closures run (node flags in `build_node`,
            // `.lss` state parts in `compute_styles`, focus ring/caret in
            // paint), all of which a memoized rebuild re-does for every node
            // regardless. Pointer motion therefore gets F1-memoized rebuilds
            // instead of unmemoized O(tree) ones. Guarded by
            // tests/hover_memo.rs; if visual state ever becomes readable from
            // `BuildCx`, it must be signal-backed so scopes record the read.
            if self.force_rebuild {
                self.clear_view_caches();
            }
            // A.3.2: visual-state rebuilds must re-resolve `.lss` state parts
            // (`:hovered` etc.) for every node, so they never copy spans
            // forward; signal/time rebuilds may. A.3.5: LUMEN_FULL_REBUILD=1
            // is the bisect hatch — naive full rebuilds, no retained reuse.
            // AN1: animations no longer veto copy-forward for the WHOLE app.
            // `anims_active()` is a single global bool, so one hover fade
            // anywhere turned memoization off for every scope on screen — and
            // transitions are the common case once an app uses them at all, so
            // CP1/CP2's wins would never have shown up in a real UI. The check
            // moved into `copy_span`, which already enumerates a span's nodes
            // and can ask the narrower question: is anything *in this span*
            // animating?
            let _ = anims_running;
            self.allow_copy_forward = !visual_changed && !full_rebuild_forced();
            self.rebuild(); //  baselines force_rebuild + last_build_gen
        } else if restyle_only {
            // restyle_visual already updated flags/styles/semantics/paint.
            //
            // …except for text bindings. A pointer press changes visual state
            // (`pressed`), so a pump where the click ALSO moved a signal a
            // `bind!` text reads came down this arm — which never reached the
            // binding check below, and served a frame whose bound text was one
            // edit behind. `assert_view_coherent` fails on exactly that frame.
            // It healed on the next pump, so a live window hid it and only
            // headless tests (which pump once per event) saw it.
            if write_changed && !stale_text.is_empty() && !self.patch_text_bindings(&stale_text) {
                self.allow_copy_forward = !visual_changed && !full_rebuild_forced();
                self.rebuild();
            }
        } else if write_changed && !stale_text.is_empty() {
            // F3.5: a text binding changed. Patch if the new string measures
            // the same, else rebuild. MUT1: the verdict is per binding — the
            // patchable ones commit (their spans splice through the rebuild
            // and keep the values), the decliners' scope chains are evicted by
            // `settle_bindings_for_rebuild`, and everything else splices.
            if !self.patch_text_bindings(&stale_text) {
                self.allow_copy_forward = !visual_changed && !full_rebuild_forced();
                self.rebuild();
            } else if !stale_bg.is_empty() {
                // A background binding changed in the same pump — fold it in,
                // rather than leave it for a frame that may never come.
                self.patch_bg_bindings(&stale_bg);
            }
        } else if write_changed && !stale_bg.is_empty() {
            self.patch_bg_bindings(&stale_bg);
        } else {
            // Nothing changed — keep the retained frame, report no damage.
            self.last_damage = Damage::None;
            self.record_change("idle", Vec::new);
            // O4.2: "I changed state and the UI is stale" is the top entry in
            // the debugging-lumen skill and had no machine-readable trace at
            // all — `pump` computes every predicate involved and discards them.
            //
            // The trigger is deliberately NARROW. Warning on any write with no
            // view dependents would fire on every signal that keys a
            // `resource` — task deps live in `lumen-app/src/tasks.rs` and never
            // register in `m.deps`, so the canonical async pattern has zero
            // view dependents BY DESIGN. Making false positives the first thing
            // the audit ever logs in an async app would destroy trust in the
            // channel on day one. So: a view genuinely depends on this signal,
            // AND the frame still went idle. That is the
            // read-tracking-missed-a-dependency bug, not "nobody is listening".
            #[cfg(all(feature = "dev-observability", feature = "snapshot"))]
            if write_changed {
                self.warn_stale_writes();
            }
        }
        // F0 fixpoint contract: a settled pump leaves the reactive graph
        // quiescent. Writes flush synchronously, so after dispatch + build
        // nothing should stay dirty; if this fires, some effect is scheduling
        // work that never drains (a real bug, not a perf issue).
        debug_assert!(
            self.rt.is_quiescent(),
            "pump left the reactive graph non-quiescent"
        );
        let stats = FrameStats {
            node_count: self.tree.len(),
            painted: self.last_damage != Damage::None,
            damage: self.last_damage,
            nodes_rebuilt: self.nodes_rebuilt,
            nodes_copied: self.nodes_copied,
        };
        if stats.painted {
            self.last_paint_damage = (stats.damage, self.frames_rendered + 1);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if stats.painted {
            let ms = pump_t0.elapsed().as_secs_f32() * 1000.0;
            if self.frame_ms.len() >= 120 {
                self.frame_ms.pop_front();
            }
            self.frame_ms.push_back(ms);
            self.frames_rendered += 1;
            self.frame_ms_max = self.frame_ms_max.max(ms);
            if ms > FRAME_BUDGET_MS {
                self.frames_over_budget += 1;
            }
        }
        // Painted frames, plus any pump whose tree changed since the last
        // audit. `painted` alone is NOT sufficient: `set_stylesheet` and
        // friends rebuild outside `pump`, so the pump that follows a hot
        // stylesheet edit is idle — and that is exactly the moment a developer
        // most wants to hear about a new finding.
        #[cfg(feature = "dev-observability")]
        self.rt.set_log_frame(self.frames_rendered);
        // O0.15: …but not on *every* such pump. The audit is a push channel
        // for a human or an agent, and its contract is "tell me promptly", not
        // "tell me this exact frame" — while a 60 fps animation asked it 60
        // times a second, at 858 µs a pass on a 4000-node page (27% of the
        // frame, measured with the audit compiled out as the control).
        //
        // Two conditions still force a pass, and between them they cover the
        // cases a plain interval would lose:
        //
        //  * the tree is at **rest** — stale, but this pump did not itself
        //    move it. That covers two cases at once: a finding introduced
        //    during an animation is reported the moment the animation stops,
        //    and a rebuild that happened OUTSIDE `pump` (`set_stylesheet`,
        //    `set_theme`, `resize`) is reported by the very next pump, which
        //    is the case O0.3 exists for.
        //
        //    Two definitions of "settled" were wrong before this one, and both
        //    failed loudly rather than quietly, which is the only reason they
        //    were caught: `!stats.painted` fires on every frame of a rebuild
        //    whose output is pixel-identical — precisely the workload being
        //    throttled — and "same generation as the previous pump" misses the
        //    out-of-band rebuild above, breaking the O0.3 case;
        //  * nothing has been audited yet at this generation and the interval
        //    has elapsed.
        //
        // What this genuinely gives up: a finding that appears *and
        // disappears* entirely inside one interval, during continuous
        // animation, is never seen. That is the trade, and it is why the
        // interval is 100 ms rather than a frame budget — a defect that
        // survives a tenth of a second is still caught, and one that does not
        // was never actionable.
        #[cfg(feature = "dev-observability")]
        {
            let gen = self.sem_gen.get();
            // `stale` is the primary gate and is unchanged from O0.3: there is
            // nothing to say unless the tree has moved since the last audit.
            // A static app is never stale and never audits, throttle or not.
            let stale = gen != self.last_audit_gen;
            let settled = gen == gen_on_entry;
            let due = self.clock_ms - self.last_audit_ms >= Self::AUDIT_MIN_INTERVAL_MS;
            if stale && (settled || due) {
                self.last_audit_ms = self.clock_ms;
                self.ambient_audit();
            }
        }
        stats
    }

    /// O4.2: report a signal write that a view depends on and that produced no
    /// frame. See the call site in `pump` for why the condition is this narrow.
    #[cfg(all(feature = "dev-observability", feature = "snapshot"))]
    fn warn_stale_writes(&self) {
        for key in self.rt.keys_written_since(self.last_build_gen) {
            let deps = self.dependents_of(&key);
            if deps.is_empty() {
                continue; // a task/resource key, or genuinely unread — not stale.
            }
            self.rt.log(
                "warn",
                format!(
                    "signal `{key}` was written and the frame went idle, but \
                     {} node(s) read it. The view should have updated — this is \
                     a missed read-dependency, not a no-op write.",
                    deps.len()
                ),
            );
        }
    }

    /// O0.15: how long the ambient audit may go without running while the
    /// view keeps changing.
    ///
    /// A tenth of a second is below the threshold at which a developer
    /// notices a delay, and roughly 6× cheaper than per-frame at 60 fps. It is
    /// a floor on *cadence*, not a cap on fidelity: `ui.lint` answers exactly
    /// and immediately whenever asked, and a settled tree is audited at once.
    #[cfg(feature = "dev-observability")]
    const AUDIT_MIN_INTERVAL_MS: f64 = 100.0;

    /// O0.3: push newly-appeared lint findings into the log ring.
    ///
    /// This is the bridge that turns the whole lint surface from *pull* into
    /// *push*. `ui.lint` is interrogative — it answers well, but only if the
    /// caller already suspects something and asks. A human looking at a window
    /// gets the same information ambiently and without a hypothesis, and an
    /// agent driving that window had no equivalent. Running the audit each
    /// painted frame and logging the *changes* is that equivalent, and it costs
    /// nothing per new check: every lint that exists today and every one added
    /// later becomes push-mode for free.
    ///
    /// Three properties this must have, each learned the hard way:
    ///
    /// * **Painted frames only.** An idle pump changes nothing, so re-linting
    ///   it is pure waste.
    /// * **Edge-triggered per finding.** The ring holds 1000 entries; a
    ///   held finding re-logged every frame would flush it in seconds. The
    ///   diff reports a key the frame it appears and stays quiet while it
    ///   persists — and reports it again if it is fixed and reintroduced.
    /// * **Keyed on `(code, node)`, not on the message.** Messages carry
    ///   measured values ("12×0 past the edge") that jitter during an
    ///   animation, which would defeat deduplication entirely. The node comes
    ///   from `Diagnostic.handle` (O0.1b) — path-derived and always present,
    ///   unlike `node`, which is the author's optional `#id`.
    #[cfg(feature = "dev-observability")]
    fn ambient_audit(&mut self) {
        self.last_audit_gen = self.sem_gen.get();
        let findings = self.lint();
        // Key, then look the finding back up: `FrameDiff` owns the key set, and
        // cloning whole diagnostics into it would keep every message string
        // alive for a frame longer than needed.
        let keyed: Vec<(String, usize)> = findings
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let anchor = d
                    .handle
                    .as_deref()
                    .or_else(|| d.node.as_ref().map(|n| n.as_str()))
                    .unwrap_or("-");
                (format!("{}:{}", d.code, anchor), i)
            })
            .collect();
        let index: std::collections::HashMap<&str, usize> =
            keyed.iter().map(|(k, i)| (k.as_str(), *i)).collect();
        let fresh = self
            .audit_diff
            .newly_present(keyed.iter().map(|(k, _)| k.clone()));
        for key in fresh {
            let Some(&i) = index.get(key.as_str()) else {
                continue;
            };
            let d = &findings[i];
            let level = match d.severity {
                lumen_core::Severity::Error => "error",
                lumen_core::Severity::Warning => "warn",
            };
            // O4.6: keep `code` and the node anchor structured rather than
            // flattening them into prose the consumer has to re-parse.
            self.rt.log_diagnostic(level, d);
        }
    }

    /// C.4b: invoke a named app command registered by the last build
    /// (`cx.register_command`). Returns `false` (and the registered names)
    /// via `Err` when unknown; runs + pumps on success.
    pub fn run_command(&mut self, name: &str) -> Result<(), Vec<String>> {
        let Some(h) = self.commands.get(name).cloned() else {
            let mut names: Vec<String> = self.commands.keys().cloned().collect();
            names.sort();
            return Err(names);
        };
        h(&self.rt);
        self.pump();
        Ok(())
    }

    /// B.5: drive `transition:` declarations for one node. For each
    /// transitioned paint property (background/color/opacity/border-radius
    /// v1; layout properties are documented no-ops), compare the freshly
    /// resolved target with the running animation's target: a change starts
    /// a new segment *from the current blended value* (smooth interruption),
    /// and the css value is substituted with the blend so paint and probes
    /// see the animated frame. First sighting of a node adopts the target
    /// without animating (no mount flash).
    fn apply_transitions(&mut self, id: &Option<StableId>, css: &mut lumen_style::Style) {
        // B.5b: during a theme switch, color properties get an implicit
        // 150 ms transition (04 §4) unless the node declares its own.
        let theme_window = self.clock_ms < self.theme_anim_until;
        if css.transitions.is_empty() && !theme_window {
            return;
        }
        let Some(id) = id.clone() else { return };
        let now = self.clock_ms;
        let specs = if css.transitions.is_empty() {
            vec![
                lumen_style::Transition {
                    property: "background".into(),
                    duration_ms: 150.0,
                    easing: lumen_style::Easing::Ease,
                    delay_ms: 0.0,
                },
                lumen_style::Transition {
                    property: "color".into(),
                    duration_ms: 150.0,
                    easing: lumen_style::Easing::Ease,
                    delay_ms: 0.0,
                },
            ]
        } else {
            css.transitions.clone()
        };
        for tr in &specs {
            const PAINT_PROPS: [&str; 4] = ["background", "color", "opacity", "border-radius"];
            let props: Vec<&'static str> = if tr.property == "all" {
                PAINT_PROPS.to_vec()
            } else {
                PAINT_PROPS
                    .iter()
                    .copied()
                    .filter(|p| *p == tr.property)
                    .collect()
            };
            for prop in props {
                let target = match prop {
                    "background" => css.background.map(AnimVal::Color),
                    "color" => css.color.map(AnimVal::Color),
                    "opacity" => css.opacity.map(AnimVal::Num),
                    "border-radius" => css.border_radius.map(AnimVal::Num),
                    _ => None,
                };
                let Some(target) = target else { continue };
                let key = (id.clone(), prop);
                let entry = self.prop_anims.entry(key).or_insert_with(|| PropAnim {
                    from: target,
                    to: target,
                    start_ms: now,
                    duration_ms: 0.0,
                    delay_ms: 0.0,
                    easing: tr.easing,
                    committed: true,
                });
                if entry.to != target {
                    let cur = entry.value_at(now);
                    *entry = PropAnim {
                        from: cur,
                        to: target,
                        start_ms: now,
                        duration_ms: if self.reduced_motion {
                            0.0
                        } else {
                            tr.duration_ms
                        },
                        delay_ms: tr.delay_ms,
                        easing: tr.easing,
                        committed: false,
                    };
                }
                if entry.done(now) {
                    entry.committed = true;
                }
                match (prop, entry.value_at(now)) {
                    ("background", AnimVal::Color(c)) => css.background = Some(c),
                    ("color", AnimVal::Color(c)) => css.color = Some(c),
                    ("opacity", AnimVal::Num(v)) => css.opacity = Some(v),
                    ("border-radius", AnimVal::Num(v)) => css.border_radius = Some(v),
                    _ => {}
                }
            }
        }
    }

    /// B.5b: play the node's `animation:` timeline — sample the evaluated
    /// `@keyframes` stops at the current phase (iteration count, alternate,
    /// per-segment easing) and override the paint-tier css values. Fills
    /// forwards on completion (friendlier than CSS's default snap-back, and
    /// avoids an end flash; documented in 04).
    fn apply_keyframes(&mut self, id: &Option<StableId>, css: &mut lumen_style::Style) {
        let Some(spec) = css.animation.clone() else {
            return;
        };
        let Some(id) = id.clone() else { return };
        if self.reduced_motion && !css.animation_force {
            return;
        }
        let Some(stops) = self
            .style_env
            .as_ref()
            .and_then(|e| e.keyframes.get(&spec.name))
            .cloned()
        else {
            return;
        };
        if stops.is_empty() {
            return;
        }
        let now = self.clock_ms;
        let entry = self.key_anims.entry(id).or_insert((now, false));
        let e = now - entry.0 - spec.delay_ms as f64;
        if e < 0.0 {
            return;
        }
        let iter = e / spec.duration_ms.max(1.0) as f64;
        let mut phase = iter.fract() as f32;
        let mut finished = false;
        if let Some(count) = spec.count {
            if iter >= count as f64 {
                finished = true;
                phase = 1.0;
            }
        }
        if spec.alternate && (iter as u64) % 2 == 1 && !finished {
            phase = 1.0 - phase;
        }
        entry.1 = finished;

        let sample_color = |get: fn(&KeyStop) -> Option<Color>| -> Option<Color> {
            sample_stops(&stops, phase, spec.easing, |st| get(st).map(AnimVal::Color)).map(|v| {
                match v {
                    AnimVal::Color(c) => c,
                    AnimVal::Num(_) => unreachable!(),
                }
            })
        };
        let sample_num = |get: fn(&KeyStop) -> Option<f32>| -> Option<f32> {
            sample_stops(&stops, phase, spec.easing, |st| get(st).map(AnimVal::Num)).map(
                |v| match v {
                    AnimVal::Num(n) => n,
                    AnimVal::Color(_) => unreachable!(),
                },
            )
        };
        if let Some(c) = sample_color(|st| st.background) {
            css.background = Some(c);
        }
        if let Some(c) = sample_color(|st| st.color) {
            css.color = Some(c);
        }
        if let Some(v) = sample_num(|st| st.opacity) {
            css.opacity = Some(v);
        }
        if let Some(v) = sample_num(|st| st.border_radius) {
            css.border_radius = Some(v);
        }
    }

    /// B.5: whether any transition or keyframe timeline is still running.
    fn anims_active(&self) -> bool {
        self.prop_anims.values().any(|a| !a.committed)
            || self.key_anims.values().any(|(_, done)| !done)
    }

    /// B.5: clamp all transition durations to zero (the OS reduced-motion
    /// signal, 04 §3). The shell wires the real OS setting (P-phase); tests
    /// and apps can set it directly.
    pub fn set_reduced_motion(&mut self, on: bool) {
        self.reduced_motion = on;
        if on {
            // Stop anything mid-flight and re-resolve to base values —
            // toggling the OS setting takes effect immediately.
            self.prop_anims.clear();
            self.key_anims.clear();
            self.theme_anim_until = 0.0;
            self.force_rebuild = true;
            self.pump();
        }
    }

    /// A.5b introspection: cumulative style-resolution memo `(hits, misses)`
    /// — most nodes share a handful of (desc, ancestors) keys, so hits should
    /// dwarf misses on any real UI.
    /// O1.3: everything `app.perf` reports, in one read.
    ///
    /// Deliberately a struct rather than more tuple-returning accessors:
    /// `perf_stats` already returns an unlabelled `(f64, f64, u64)`, and the
    /// surface is about to carry a dozen numbers whose meanings are easy to
    /// transpose.
    pub fn perf_report(&self) -> PerfReport {
        let (p50, p95, frames) = self.perf_stats();
        let (memo_hits, memo_misses) = self.style_memo_stats();
        let (shape_len, shape_cap, run_len, run_cap) = self.text.cache_stats();
        PerfReport {
            frame_ms_p50: p50,
            frame_ms_p95: p95,
            frame_ms_max: self.frame_ms_max as f64,
            frames_rendered: frames,
            frames_over_budget: self.frames_over_budget,
            frame_budget_ms: FRAME_BUDGET_MS as f64,
            nodes_rebuilt_total: self.nodes_rebuilt_total,
            nodes_copied_total: self.nodes_copied_total,
            style_memo_hits: memo_hits,
            style_memo_misses: memo_misses,
            shape_cache_len: shape_len,
            shape_cache_cap: shape_cap,
            run_cache_len: run_len,
            run_cache_cap: run_cap,
            renderer: self.renderer.name(),
            is_gpu: self.renderer.is_gpu(),
            backend: self.renderer.backend(),
            backend_has_known_defects: self.renderer.backend_has_known_defects(),
        }
    }

    /// A.5b: style-memo `(hits, misses)`, cumulative for the life of the
    /// runtime. Surfaced through [`Headless::perf_report`] / `app.perf`.
    pub fn style_memo_stats(&self) -> (u64, u64) {
        (self.style_memo_hits, self.style_memo_misses)
    }

    /// C.2 (`app.perf`): rolling painted-frame time percentiles
    /// `(p50_ms, p95_ms)` over the last ≤120 painted pumps, plus the total
    /// painted-frame count. Zeros before anything painted (and on wasm).
    pub fn perf_stats(&self) -> (f64, f64, u64) {
        let mut v: Vec<f32> = self.frame_ms.iter().copied().collect();
        if v.is_empty() {
            return (0.0, 0.0, self.frames_rendered);
        }
        v.sort_by(f32::total_cmp);
        let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize] as f64;
        (pct(0.50), pct(0.95), self.frames_rendered)
    }

    /// Enqueue an event (OS or synthesized — same path).
    pub fn inject(&mut self, ev: Event) {
        self.input.push(ev);
    }

    /// W.0: the node a custom leaf's `event()` would be offered `ev` at —
    /// the hit-test target for pointer events, the focused node for
    /// keyboard/text. `None` for events leaves don't receive directly.
    fn leaf_event_target(&self, ev: &Event) -> Option<NodeIndex> {
        match ev {
            Event::PointerDown(pe) | Event::PointerUp(pe) | Event::PointerMove(pe) => {
                self.tree.hit_test(pe.pos)
            }
            Event::Wheel(we) => self.tree.hit_test(we.pos),
            Event::KeyDown(_) | Event::KeyUp(_) | Event::TextInput(_) => self.focused_node(),
            _ => None,
        }
    }

    /// Resize the render surface. Updates the size used for layout *and*
    /// rasterization, then re-lays-out and repaints so hit-test bounds and the
    /// rendered frame both track the new dimensions. The desktop shell calls
    /// this on `WindowEvent::Resized`; without it, layout (hence every node's
    /// hit rectangle) stays at the old size and the old-size frame gets
    /// upscaled by the presenter (blur). No-op if the size is unchanged.
    pub fn resize(&mut self, size: Size) {
        if size != self.size {
            self.size = size;
            self.force_rebuild = true; // layout changed; not a signal write
            self.pump();
        }
    }

    /// Set logical size + HiDPI scale **without** repainting — the caller pumps
    /// once afterward. Lets the shell apply a coalesced resize and render the new
    /// size in a single pump per frame (instead of `resize()` + `set_scale()`
    /// each pumping, then another `pump()`). No-op-safe; ignores non-positive
    /// scale.
    pub fn prepare_resize(&mut self, size: Size, scale: f64) {
        if size != self.size || (scale > 0.0 && scale != self.scale) {
            self.force_rebuild = true; // make the following pump rebuild
        }
        self.size = size;
        if scale > 0.0 {
            self.scale = scale;
        }
    }

    /// The current surface size (logical px).
    pub fn size(&self) -> Size {
        self.size
    }

    /// The current HiDPI scale factor (physical px per logical px).
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Set the HiDPI scale factor and repaint at the new physical resolution.
    /// Layout (logical) is unaffected; only the rasterized frame's pixel size
    /// changes. The desktop shell calls this on `ScaleFactorChanged`. No-op if
    /// unchanged or non-positive.
    pub fn set_scale(&mut self, scale: f64) {
        if scale > 0.0 && scale != self.scale {
            self.scale = scale;
            self.force_rebuild = true; // physical raster size changed
            self.pump();
        }
    }

    /// The most recent rendered frame. With a live surface attached (1c) the
    /// build presents straight to the swapchain and no longer fills `self.frame`,
    /// so render the retained display list on demand here (the agent/test capture
    /// path — a freshly rendered frame of current state). Otherwise return the
    /// cached frame.
    pub fn screenshot(&mut self) -> RgbaImage {
        if self.surface_attached {
            if let Some(dl) = self.last_dl.take() {
                let pw = (self.size.width * self.scale).round().max(1.0) as u32;
                let ph = (self.size.height * self.scale).round().max(1.0) as u32;
                let bg = Color::srgb8(255, 255, 255, 255);
                let img = self.renderer.render_frame(&dl, pw, ph, self.scale, bg);
                self.last_dl = Some(dl);
                return img;
            }
        }
        self.frame.clone()
    }

    /// Render a magnified crop of `region` (logical px) at `scale_mul`× the normal
    /// scale, with optional debug `outlines` (rect + color, drawn as 1px borders)
    /// — e.g. a node's box and ink bounds. Lets a small defect (a clipped
    /// descender) be inspected at zoom instead of hunting for it in a full-window
    /// screenshot. Deterministic (same CPU/GPU render path); overlays are opt-in.
    pub fn screenshot_zoom(
        &mut self,
        region: kurbo::Rect,
        scale_mul: f64,
        outlines: &[(kurbo::Rect, Color)],
    ) -> RgbaImage {
        let (mut dl, _) = self.build_display_list();
        for (r, color) in outlines {
            dl.push(DrawCmd::Rect {
                rect: *r,
                brush: Brush::Solid(Color::TRANSPARENT),
                radii: CornerRadii::all(0.0),
                border: Some(Border {
                    width: 1.0,
                    color: *color,
                }),
            });
        }
        let zoom = (self.scale * scale_mul).max(0.1);
        let pw = (self.size.width * zoom).round().max(1.0) as u32;
        let ph = (self.size.height * zoom).round().max(1.0) as u32;
        let bg = Color::srgb8(255, 255, 255, 255);
        let full = self.renderer.render_frame(&dl, pw, ph, zoom, bg);
        let x0 = (region.x0 * zoom).floor().clamp(0.0, pw as f64) as u32;
        let y0 = (region.y0 * zoom).floor().clamp(0.0, ph as f64) as u32;
        let x1 = (region.x1 * zoom).ceil().clamp(0.0, pw as f64) as u32;
        let y1 = (region.y1 * zoom).ceil().clamp(0.0, ph as f64) as u32;
        full.crop(
            x0,
            y0,
            x1.saturating_sub(x0).max(1),
            y1.saturating_sub(y0).max(1),
        )
    }

    /// Wire a live window surface to the renderer for direct present (1c).
    /// Returns whether the backend accepted it (GPU present); on `false` the
    /// shell keeps the CPU readback + separate-presenter path. `width`/`height`
    /// are physical px.
    #[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
    pub fn attach_surface(
        &mut self,
        target: lumen_render::wgpu::SurfaceTarget<'static>,
        width: u32,
        height: u32,
    ) -> bool {
        let ok = self.renderer.attach_surface(target, width, height);
        self.surface_attached = ok;
        ok
    }

    /// Reconfigure the attached surface to a new physical size (1c).
    #[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
    pub fn resize_surface(&mut self, width: u32, height: u32) {
        self.renderer.resize_surface(width, height);
    }

    /// Present the most recent frame straight to the attached swapchain (1c) —
    /// no CPU readback. The shell calls this after `pump()` when the frame
    /// changed; see [`Present`] for what each outcome obliges it to do.
    #[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
    pub fn present_to_surface(&mut self) -> Present {
        if !self.surface_attached {
            return Present::Unavailable;
        }
        // No display list yet is a *timing* miss, not a dead surface — the very
        // distinction this return type exists to keep. Reporting it as
        // unavailable would degrade the whole session to CPU readback the first
        // time the shell asked for a present before the first paint.
        let Some(dl) = self.last_dl.take() else {
            return Present::Skipped;
        };
        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
        let bg = Color::srgb8(255, 255, 255, 255);
        // R6.2/R6.3: hand the backend the damaged region so it can cull the
        // display list and scissor the redraw. Only `Region` qualifies —
        // `Damage::Full` and `Damage::None` both mean "no usable sub-region",
        // and the backend falls back to a full frame. Anything that invalidates
        // the retained target (resize, surface recreation) already routes
        // through `force_full_repaint`, which clears `last_dl` and so produces
        // `Full` on the next pump.
        let dirty = match self.last_damage {
            Damage::Region(r) => Some(kurbo::Rect::new(
                (r.x0 * self.scale).floor().max(0.0),
                (r.y0 * self.scale).floor().max(0.0),
                (r.x1 * self.scale).ceil().min(pw as f64),
                (r.y1 * self.scale).ceil().min(ph as f64),
            )),
            Damage::Full | Damage::None => None,
        };
        let outcome = self
            .renderer
            .present_to_surface(&dl, pw, ph, self.scale, bg, dirty);
        // Kept even on a skip, so the retry next frame has something to show
        // without forcing a full rebuild.
        self.last_dl = Some(dl);
        outcome
    }

    /// Force the next paint to repaint the whole frame instead of only the
    /// damaged region (R2). The shell calls this when the retained frame can't be
    /// trusted — e.g. after the surface is recreated; tests use it to compare the
    /// incremental result against a from-scratch render.
    pub fn force_full_repaint(&mut self) {
        self.last_dl = None;
        self.force_rebuild = true;
        self.pump();
    }

    /// The damage applied by the most recent paint (R2).
    pub fn last_damage(&self) -> Damage {
        self.last_damage
    }

    /// The most recent damage that actually **painted**, and the frame number
    /// it painted on.
    ///
    /// Distinct from [`last_damage`](Self::last_damage), which is per-pump and
    /// resets to `None` on every idle frame. That distinction does not matter
    /// headless, where nothing pumps between an action and the query — but
    /// under a live shell the winit loop pumps continuously, so by the time an
    /// agent asks "what did my click repaint" the answer has already been
    /// overwritten by an idle frame. Found exactly that way: a click that
    /// demonstrably painted (`frames_rendered` 0 → 1) reported `kind: none`.
    ///
    /// The frame number lets a caller tell a fresh answer from a stale one.
    pub fn last_paint_damage(&self) -> (Damage, u64) {
        self.last_paint_damage
    }

    /// Capture a tier-3 [`AppSnapshot`] (reactive store + focus) for a later
    /// restart via [`App::run_headless_restored`]. Snapshot builds only.
    #[cfg(feature = "snapshot")]
    pub fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            state: self.rt.snapshot(),
            focused: self.focused_id.clone(),
        }
    }

    /// The current virtual-clock time (ms).
    pub fn now_ms(&self) -> f64 {
        self.clock_ms
    }

    /// C.1b: whether the last build declared future time-driven work — an
    /// `animate()` (continuous) request or a `wake_at` still ahead of the
    /// clock. `false` = settled: no scheduled frame will differ until some
    /// other input changes. A bare `now_ms()` read does **not** count: such
    /// a frame is a function of time but schedules nothing, so there is no
    /// event to wait for.
    pub fn is_time_driven(&self) -> bool {
        self.requests.continuous
            || self.requests.wakes.iter().any(|w| *w > self.clock_ms)
            || self.anims_active()
    }

    /// The reactive runtime backing this app (state store + scheduler). Lets
    /// tests/tools read `write_gen`/`is_quiescent` and drive signals directly.
    pub fn runtime(&self) -> &Runtime {
        &self.rt
    }

    /// The pointer shape as a stable name — `"pointer"`, `"text"`,
    /// `"col-resize"`, … or `"default"` when no rule applies.
    ///
    /// The cursor is user-visible state that no screenshot captures, so
    /// without this an agent or a test cannot tell whether a control advertises
    /// itself as draggable or typeable. `"default"` rather than `null` for the
    /// no-rule case, because that is what the shell shows.
    pub fn cursor_name(&self) -> &'static str {
        use lumen_core::CursorShape as C;
        match self.cursor_shape().unwrap_or(C::Default) {
            C::Default => "default",
            C::Pointer => "pointer",
            C::Text => "text",
            C::Wait => "wait",
            C::Crosshair => "crosshair",
            C::Move => "move",
            C::ColResize => "col-resize",
            C::RowResize => "row-resize",
            C::NotAllowed => "not-allowed",
            C::None => "none",
        }
    }

    /// PROP1: the pointer shape for whatever the pointer is currently over.
    ///
    /// Resolved from the hovered node's `cursor` — the `.lss` rule if there is
    /// one, else the widget's own — walking ancestors, because CSS `cursor`
    /// inherits and a button's label should not punch a hole in the button's
    /// own pointer. `None` means "no rule applies", which the shell renders as
    /// the platform default; see [`cursor_name`](Self::cursor_name) for the
    /// resolved name.
    ///
    /// Lives here rather than in the shell because hit-testing and the resolved
    /// style are both runtime state; the shell only maps the shape to its
    /// platform's name.
    pub fn cursor_shape(&self) -> Option<lumen_core::CursorShape> {
        let mut node = self.hovered_node()?;
        loop {
            // `.lss` wins; the element's own shape is the widget's default.
            if let Some(c) = self
                .node_style
                .get(&node)
                .and_then(|s| s.cursor)
                .or_else(|| self.meta.get(&node).and_then(|m| m.cursor))
            {
                return Some(c);
            }
            let parent = self.tree.parent(node);
            if parent == NodeIndex::NONE {
                return None;
            }
            node = parent;
        }
    }

    /// The node under the pointer, if any (the hit-test result behind
    /// [`cursor_shape`](Self::cursor_shape)).
    fn hovered_node(&self) -> Option<NodeIndex> {
        let id = self.hovered_id.as_ref()?;
        self.meta
            .iter()
            .find(|(_, m)| m.id.as_ref() == Some(id))
            .map(|(n, _)| *n)
    }

    /// Advance the virtual clock by `ms`.
    pub fn advance_clock(&mut self, ms: f64) {
        self.clock_ms += ms;
    }

    /// Advance the virtual clock by `dt_ms` and pump one frame. The deterministic
    /// driver for time-based UI: a test calls `advance(1000.0)` to move a clock
    /// hand exactly one second; the desktop shell calls it with the real elapsed
    /// time each frame. Equivalent to [`advance_clock`](Self::advance_clock) then
    /// [`pump`](Self::pump).
    pub fn advance(&mut self, dt_ms: f64) -> FrameStats {
        self.advance_clock(dt_ms);
        self.pump()
    }

    /// Whether the latest build requested continuous animation (via
    /// [`BuildCx::animate`](crate::BuildCx::animate)).
    pub fn is_animating(&self) -> bool {
        self.requests.continuous
    }

    /// The next virtual-clock time (ms) at which the UI wants a frame, or `None`
    /// if it is idle. `Some(t)` with `t <= now_ms()` means "animate now" (a
    /// continuous animation); a larger `t` is a one-shot wake. The host turns
    /// this into a wait/poll decision so an idle UI costs no frames.
    pub fn next_deadline(&self) -> Option<f64> {
        // A coast is time-driven and is not expressed by any build's requests,
        // so it has to ask for frames itself or momentum would stop the instant
        // the UI went otherwise idle.
        if self.requests.continuous || self.fling.is_some() {
            return Some(self.clock_ms);
        }
        self.requests
            .wakes
            .iter()
            .copied()
            .filter(|t| *t > self.clock_ms)
            .min_by(|a, b| a.total_cmp(b))
    }

    /// The semantics document as JSON (`lumen-semantics/1`, 03 §1). Snapshot
    /// builds only (the agent introspection path).
    #[cfg(feature = "snapshot")]
    pub fn semantics_json(&self) -> serde_json::Value {
        (*self.semantics_json_cached(false)).clone()
    }

    /// Structured diagnostics for the current frame (e.g. `W0103` layout
    /// overflow). Lets an agent detect and fix layout bugs by code.
    pub fn diagnostics(&self) -> Vec<lumen_core::Diagnostic> {
        // `sem_root()` is a memoized `Rc` — `semantics_doc()` deep-clones the
        // whole tree (a String/Vec allocation per node) purely to hand out a
        // reference the audits then only read.
        let mut diags = crate::audit::lint(&self.sem_root());
        if let Some(d) = &self.build_panic {
            diags.push(d.clone());
        }
        diags
    }

    /// The absolute visual-invariant lint (overflow / clipping / zero-area
    /// interactive) over the current tree — see [`audit::lint`](crate::audit::lint).
    /// Unlike goldens, catches first-time layout/render defects; usable in tests
    /// and via the agent (`ui.lint`).
    ///
    /// Capped at [`MAX_PER_CODE`](Self::MAX_PER_CODE) findings per code, with
    /// the remainder summarised — see [`lint_all`](Self::lint_all) when the
    /// caller genuinely wants every one.
    pub fn lint(&mut self) -> Vec<lumen_core::Diagnostic> {
        self.lint_capped(Self::MAX_PER_CODE)
    }

    /// [`lint`](Self::lint) with **no per-code cap** — every finding, however
    /// many.
    ///
    /// O0.5 capped the ambient audit because it runs on a frame budget and a
    /// long page produced thousands of findings a frame, which is a formatting
    /// cost paid to say one thing repeatedly. A caller who asked for a lint and
    /// is waiting for the answer is in the opposite position: the cost is
    /// bounded by the one call, and a cap could hide the node they are looking
    /// for. So the cap belongs to the *ambient* pass, not to the check — this
    /// is the explicit, pull-mode path (`ui.lint {"all": true}`).
    ///
    /// Note the cap was never the expensive half. O0.3a made the underlying
    /// scans cheap (cached shaping, borrowed semantics root); the cap only
    /// bounds message *formatting*, which is why removing it for a single call
    /// is affordable and removing it for every frame was not.
    pub fn lint_all(&mut self) -> Vec<lumen_core::Diagnostic> {
        self.lint_capped(usize::MAX)
    }

    fn lint_capped(&mut self, cap: usize) -> Vec<lumen_core::Diagnostic> {
        // See `diagnostics()`: borrow the memoized root, don't clone the tree.
        let mut out = crate::audit::lint(&self.sem_root());
        // What the renderer actually clamped this frame (W0110). The CPU
        // backend returns nothing; only a GPU one has limits to hit.
        out.extend(self.renderer.take_diagnostics());
        // O5.2: the text engine latches its own regime changes the same way —
        // it has no `Runtime` handle, so it reports upward and the app drains.
        out.extend(self.text.take_diagnostics());
        // SD4: W0106 (a node advertises a semantic Action it does not
        // implement) was emitted only by `audit_actions()`, which tests call
        // and `ui.lint` never did — so an agent could not observe this class of
        // defect at all, even though the whole point of the action list is
        // that the agent and assistive tech read it as a contract. Surfaced by
        // SD5.0's registry repair: the code existed, was documented, and had
        // no reachable path.
        out.extend(self.audit_actions());
        // W0110 portability advisory. Checked against a FIXED 2048 px — the
        // WebGL2/downlevel floor — regardless of which backend is running,
        // because an element needing a bigger sprite is non-portable whatever
        // GPU the author happens to own. That is precisely the class a CPU-only
        // test suite cannot see: the shadow that crashed a live GPU window
        // rendered perfectly in every headless test.
        //
        // Emitted here rather than in `audit.rs` because `SemanticsNode` carries
        // `bounds` but not `shadow`, so a semantics walk cannot see the cause.
        const PORTABLE_TEXTURE_LIMIT: f64 = 2048.0;
        for (node, m) in self.meta.iter() {
            let Some(sh) = m.shadow().copied() else {
                continue;
            };
            let b = self.tree.bounds(*node);
            // Mirrors the sprite sizing in `paint`: the 9-slice bounds each axis
            // by style, so only a shadow that is *itself* enormous can trip this
            // — a blur of 700 px, not a tall card.
            let radius = m.corner_radius;
            let inv = (radius + sh.spread).max(0.0) + 3.0 * sh.blur.max(0.0) + 1.0;
            let margin = (sh.spread.max(0.0) + sh.blur).ceil() + 2.0;
            let side = |len: f64| len.min(2.0 * inv + 1.0) + 2.0 * margin;
            let (sw, sh_px) = (side(b.width()), side(b.height()));
            if sw > PORTABLE_TEXTURE_LIMIT || sh_px > PORTABLE_TEXTURE_LIMIT {
                let who =
                    m.id.as_ref()
                        .map(|i| format!("`#{}`", i.as_str()))
                        .unwrap_or_else(|| "an element".to_string());
                let d = lumen_core::Diagnostic::new(
                    lumen_core::codes::W0110,
                    format!(
                        "{who} is {:.0}x{:.0} px with a shadow whose sprite is \
                         {sw:.0}x{sh_px:.0} px, past the {PORTABLE_TEXTURE_LIMIT:.0} px \
                         portable texture limit. It may render on your GPU and \
                         be downscaled on a user's. Reduce the blur or spread.",
                        b.width(),
                        b.height()
                    ),
                );
                let d = match self.handle_for_index(node.index()) {
                    Some(h) => d.with_target(h.to_wire(), m.id.as_ref()),
                    None => d,
                };
                out.push(d);
            }
        }
        // T.4 tofu: any text node whose shaped block contains `.notdef`
        // glyphs (chars no registered face covers).
        //
        // O0.12: this used to clone every text node's string, `TextStyle` and
        // id into a `Vec` before looking any of them up — 4000 `String`
        // allocations a frame on a 4000-row page — purely to release the
        // `&self.meta` borrow before calling `&mut self.text`. The two are
        // disjoint *fields*, so destructuring lets both borrows live at once
        // and the staging vector disappears. What survives the scan is only
        // the offenders, which is nearly always nothing, and the second pass
        // formats those against `&self` (it needs `handle_for_index`).
        let mut offenders: Vec<(NodeIndex, Option<StableId>, String, usize)> = Vec::new();
        // O0.12: the walk below asks the shape cache about every text node, and
        // a hit is ~286 ns because the cached values are large and the lookup
        // is memory-bound — 32% of a 4000-row frame spent concluding that
        // nothing was wrong. Whether anything IS wrong is decided once, when a
        // run is shaped, so ask that instead and skip the walk entirely while
        // the answer is no. `tofu_seen` is never cleared, so the walk returns
        // the moment a real offender exists and stays exact from then on.
        if self.text.tofu_seen() {
            let Self { meta, text, .. } = self;
            for (node, m) in meta.iter() {
                let NodeContent::Text(t, ts) = &m.content else {
                    continue;
                };
                if t.is_empty() {
                    continue;
                }
                // `shaped`, NOT `layout`. `layout` bypasses the cache entirely
                // — it is the uncached primitive `shaped_by_key` calls on a
                // miss — so this loop used to re-shape every text node in the
                // tree from scratch on every `lint()`, under a comment claiming
                // the opposite ("Shaping hits the cache, so this is a cheap
                // walk"). `ShapeKey` hashes exactly (text, style, wrap, align),
                // which is what we hold here, so build/paint have already
                // populated this entry for the current frame and this is an
                // O(1) hit.
                let missing = text.shaped(t, ts, m.wrap_width, ts.align).missing_glyphs();
                if missing > 0 {
                    offenders.push((*node, m.id.clone(), t.clone(), missing));
                }
            }
        }
        let mut tofu_reported = 0usize;
        let mut tofu_suppressed = 0usize;
        for (node, id, t, missing) in offenders {
            // O0.5: a page in an unsupported script is one missing font,
            // not a thousand defects. The scan itself still visits every
            // run — its cost is the (cached) shape lookup, not the
            // reporting, and skipping runs would miss real tofu.
            if tofu_reported >= cap {
                tofu_suppressed += 1;
                continue;
            }
            tofu_reported += 1;
            let d = lumen_core::Diagnostic::new(
                lumen_core::diagnostics::codes::W0402,
                format!(
                    "tofu: {missing} glyph(s) in {t:?} not covered by any \
                     registered font — register a wider face \
                     (`App::font(bytes)`) or enable `pan-unicode`"
                ),
            );
            let d = match self.handle_for_index(node.index()) {
                Some(h) => d.with_target(h.to_wire(), id.as_ref()),
                None => d,
            };
            out.push(d);
        }
        if tofu_suppressed > 0 {
            out.push(Self::suppressed_note(
                lumen_core::diagnostics::codes::W0402,
                tofu_reported,
                tofu_suppressed,
                "text runs with uncovered glyphs",
            ));
        }
        // W0303: text that cannot be read at all. `contrast_report` already
        // measures APCA against the *composited* backdrop and binds each
        // finding to an agent handle; it simply had no caller on this path,
        // so the defect an agent most obviously cannot see (white on white)
        // was the one it could not report. `.ai_docs/03 §ui.lint` has claimed
        // this coverage since before it existed.
        //
        // A LEGIBILITY floor, not a design opinion: `ContrastLevel` keeps the
        // graded tiers for callers grading a palette. Below `LEGIBILITY_FLOOR`
        // the text is invisible, which is a defect on any design.
        out.extend(self.invisible_findings(cap));
        out.extend(self.offscreen_findings(cap));
        out.extend(self.blank_frame_findings());
        out.extend(self.occlusion_findings());
        out.extend(self.truncation_findings(cap));
        out.extend(self.stuck_animation_findings());
        out.extend(self.contrast_findings());
        out.extend(self.indefinite_shaping_findings());
        out
    }

    /// O2.1: a node's **effective** opacity — its own multiplied by every
    /// enclosing layer's.
    ///
    /// This value does not exist anywhere else in the runtime. Paint emits
    /// nested `DrawCmd::PushLayer { opacity }` and lets the backend composite
    /// them, so the multiplicative result is an emergent property of drawing,
    /// never a number that is stored. `node_style[n].opacity` is only the
    /// node's *own* post-blend value — a label at `opacity: 1` inside a group
    /// at `opacity: 0` is completely invisible and reports 1.0 there.
    ///
    /// Overlay roots reset the product to 1.0: a sheet or popup paints anchored
    /// to the window rather than under its structural parent, so it does not
    /// inherit a faded ancestor's alpha. Without this an overlay above a
    /// dimmed page would report itself as invisible while being the one thing
    /// on screen the user can actually see.
    fn effective_opacity(&self, node: NodeIndex) -> f32 {
        let mut acc = 1.0f32;
        let mut n = node;
        loop {
            if let Some(o) = self.node_style.get(&n).and_then(|s| s.opacity) {
                acc *= o.clamp(0.0, 1.0);
            }
            if self.meta.get(&n).is_some_and(|m| m.overlay) {
                break;
            }
            let parent = self.tree.parent(n);
            if !parent.is_some() || parent == n {
                break;
            }
            n = parent;
        }
        acc
    }

    /// W0403: text painted truncated while the semantic label stays full.
    ///
    /// The signal was already stored — `NodeMeta.display_text` is *"the
    /// truncated string the PAINT pass draws"*, so `Some(_)` **is** the
    /// truncation flag. Its own doc comment defends keeping `label` full,
    /// and that reasoning is right: truncating the tree would make
    /// `ui.getTree` report `"Some long lab…"` and corrupt the observability
    /// surface to fix a visual one. What was missing is the third option —
    /// keep the label full *and* say that the paint differs.
    ///
    /// Advisory: truncation is usually intentional. It rings as `warn` because
    /// `Diagnostic::new` infers severity from the code's leading letter and a
    /// `W` code cannot be `info`; the advisory nature lives in the wording, not
    /// in a severity the type system cannot express.
    fn truncation_findings(&self, cap: usize) -> Vec<lumen_core::Diagnostic> {
        let mut out = Vec::new();
        // O0.5: see MAX_PER_CODE — count past the cap, stop formatting.
        let mut suppressed = 0usize;
        for (node, m) in self.meta.iter() {
            let Some(painted) = &m.display_text else {
                continue;
            };
            let NodeContent::Text(full, _) = &m.content else {
                continue;
            };
            if painted == full {
                continue;
            }
            let who =
                m.id.as_ref()
                    .map(|i| format!("`#{}`", i.as_str()))
                    .unwrap_or_else(|| "a text node".to_string());
            if out.len() >= cap {
                suppressed += 1;
                continue;
            }
            let d = lumen_core::Diagnostic::new(
                lumen_core::codes::W0403,
                format!(
                    "{who} paints {painted:?} but its label is {full:?}. The \
                     semantic tree deliberately keeps the full string, so text \
                     assertions pass while the user sees the truncation — widen \
                     the box if that is not intended."
                ),
            );
            let d = match self.handle_for_index(node.index()) {
                Some(h) => d.with_target(h.to_wire(), m.id.as_ref()),
                None => d,
            };
            out.push(d);
        }
        out.sort_by(|a, b| a.message.cmp(&b.message));
        if suppressed > 0 {
            out.push(Self::suppressed_note(
                lumen_core::codes::W0403,
                out.len(),
                suppressed,
                "truncated text runs",
            ));
        }
        out
    }

    /// One in-flight animation, as reported by `ui.animations` (O3.3).
    ///
    /// A human sees motion. An agent takes a screenshot mid-transition and
    /// compares it to a golden with no way to know it caught a frame in
    /// flight — `is_animating()` and `next_deadline()` existed on this type and
    /// appeared nowhere in `lumen-agent`, and `ui.waitSettled` uses the
    /// underlying condition without ever reporting *what* is moving.
    pub fn animations(&self) -> Vec<AnimationInfo> {
        let now = self.clock_ms;
        let mut out: Vec<AnimationInfo> = Vec::new();
        for ((id, prop), a) in self.prop_anims.iter() {
            if a.committed {
                continue;
            }
            let total = (a.delay_ms + a.duration_ms) as f64;
            let elapsed = now - a.start_ms;
            out.push(AnimationInfo {
                node: id.as_str().to_string(),
                property: prop,
                progress: a.progress(now) as f64,
                remaining_ms: (total - elapsed).max(0.0),
                infinite: false,
                // A transition's duration is declared, so "should have finished
                // by now" is answerable from the animation itself rather than
                // from a magic constant.
                overdue_ms: (elapsed - total).max(0.0),
            });
        }
        for (id, (start, done)) in self.key_anims.iter() {
            if *done {
                continue;
            }
            out.push(AnimationInfo {
                node: id.as_str().to_string(),
                property: "animation",
                progress: 0.0,
                remaining_ms: 0.0,
                // A keyframe timeline still running has not hit its iteration
                // count — including `infinite`, where there is none. Never
                // overdue: an `animation: spin infinite` is working as declared
                // for any duration, and warning on it would fire on every
                // loading spinner in existence.
                infinite: true,
                overdue_ms: 0.0,
            });
            let _ = start;
        }
        out.sort_by(|a, b| a.node.cmp(&b.node).then_with(|| a.property.cmp(b.property)));
        out
    }

    /// How far past its own declared duration a transition must run before it
    /// is called stuck. Generous, because a busy frame legitimately overshoots.
    const ANIM_OVERDUE_MS: f64 = 2000.0;

    /// W0116: a **finite** animation that has run well past its declared total.
    ///
    /// Self-calibrating: the threshold is the animation's own duration plus
    /// slack, not a global constant. Infinite keyframe timelines are exempt by
    /// construction — a spinner is doing exactly what it was told to, for as
    /// long as it is told to, and "this is taking too long" is a question about
    /// the work behind it, not about the animation.
    fn stuck_animation_findings(&self) -> Vec<lumen_core::Diagnostic> {
        self.animations()
            .into_iter()
            .filter(|a| !a.infinite && a.overdue_ms > Self::ANIM_OVERDUE_MS)
            .map(|a| {
                lumen_core::Diagnostic::new(
                    lumen_core::codes::W0116,
                    format!(
                        "`#{}` has been transitioning `{}` for {:.0} ms past its \
                         declared duration and has not settled. Whatever it is \
                         fading toward is not arriving.",
                        a.node, a.property, a.overdue_ms
                    ),
                )
            })
            .collect()
    }

    /// O3.2: the paint-tier values this node is currently drawn with, as
    /// opposed to the cascade result `get_styles` reports.
    ///
    /// The two diverge in exactly one place, and it is a place authors hit
    /// constantly: `apply_transitions` and `apply_keyframes` substitute the
    /// mid-flight blend into `css` **before** the split at the end of the
    /// cascade (`node_style` gets `css`, `node_computed` gets `resolved`).
    /// `get_styles` reads `node_computed`, so during a 300 ms fade it reports
    /// the *target* colour while the node paints something else entirely —
    /// "why is this blue when my stylesheet says red" with no way to answer it.
    ///
    /// Deliberately a **separate method** rather than an extra key on
    /// `get_styles`: that response is a flat map of property name to value, so
    /// adding a sibling key would make `applied` look like a CSS property to
    /// anything iterating it.
    ///
    /// Reports the four properties transitions can animate (`PAINT_PROPS`);
    /// everything else is identical to the computed value by construction.
    ///
    /// Snapshot builds only, matching `get_styles` — it is the same JSON
    /// introspection surface, and `serde_json` is not linked in a lean build.
    #[cfg(feature = "snapshot")]
    pub fn applied_styles(&self, selector: &str) -> serde_json::Value {
        let root = self.semantics_elided();
        let Ok(id) = lumen_core::semantics::resolve_one(&root, selector) else {
            return serde_json::Value::Null;
        };
        let Some(node) = self.node_for_handle(id) else {
            return serde_json::Value::Null;
        };
        let Some(st) = self.node_style.get(&node) else {
            return serde_json::Value::Null;
        };
        let mut map = serde_json::Map::new();
        if let Some(c) = st.background {
            map.insert("background".into(), serde_json::json!(c.to_hex()));
        }
        if let Some(c) = st.color {
            map.insert("color".into(), serde_json::json!(c.to_hex()));
        }
        if let Some(o) = st.opacity {
            map.insert("opacity".into(), serde_json::json!(o));
        }
        if let Some(r) = st.border_radius {
            map.insert("border-radius".into(), serde_json::json!(r));
        }
        // Whether any of this is currently mid-blend, so a caller can tell
        // "the cascade says something else" from "the cascade is being
        // interpolated toward something else".
        let animating = self
            .meta
            .get(&node)
            .and_then(|m| m.id.as_ref())
            .is_some_and(|id| {
                self.key_anims.get(id).is_some_and(|(_, done)| !done)
                    || ["background", "color", "opacity", "border-radius"]
                        .iter()
                        .any(|p| {
                            self.prop_anims
                                .get(&(id.clone(), *p))
                                .is_some_and(|a| !a.committed)
                        })
            });
        map.insert("animating".into(), serde_json::json!(animating));
        serde_json::Value::Object(map)
    }

    /// The string this node actually PAINTS, when it differs from its label
    /// (`text-overflow: ellipsis`). `None` when paint and label agree.
    ///
    /// Surfaced through `ui.getLayout` beside `ink` and `opacity` — the
    /// per-node visual facts the tree deliberately does not carry.
    pub fn node_painted_text(&self, selector: &str) -> Option<String> {
        let root = self.semantics_elided();
        let id = lumen_core::semantics::resolve_one(&root, selector).ok()?;
        let node = self.node_for_handle(id)?;
        let m = self.meta.get(&node)?;
        let painted = m.display_text.as_ref()?;
        match &m.content {
            NodeContent::Text(full, _) if painted != full => Some(painted.clone()),
            _ => None,
        }
    }

    /// Fraction of an interactive node's box that must be covered before the
    /// occlusion check calls it hidden. Partial overlap is routine layout;
    /// near-total coverage means the control cannot be seen or reached.
    const OCCLUSION_COVERAGE: f64 = 0.90;

    /// Past this many nodes the occlusion scan is skipped rather than run.
    /// It is O(interactive x candidates), which is fine for real screens and
    /// not worth paying on a 10k-node data grid in a debug build.
    const OCCLUSION_NODE_CEILING: usize = 4000;

    /// W0113: an interactive node covered by something painted over it.
    ///
    /// In `lint()` rather than `audit.rs` because it needs paint order and
    /// per-node style (background alpha, opacity) keyed by `NodeIndex` — the
    /// same reason W0110 and W0402 live here. A `SemanticsNode` walk can see
    /// bounds and nothing about what was drawn into them.
    fn occlusion_findings(&self) -> Vec<lumen_core::Diagnostic> {
        let order = self.tree.paint_order();
        if order.len() > Self::OCCLUSION_NODE_CEILING {
            // Say so. A silent cap reads as "checked everything, found
            // nothing", which is the one answer this must never give.
            self.rt.log(
                "info",
                format!(
                    "occlusion check skipped: {} nodes exceeds the {} ceiling",
                    order.len(),
                    Self::OCCLUSION_NODE_CEILING
                ),
            );
            return Vec::new();
        }
        let viewport = Rect::new(0.0, 0.0, self.size.width, self.size.height);
        // Painted box of a candidate cover: its own bounds intersected with
        // every clipping ancestor and the window. Without this a panel that is
        // itself mostly scrolled away would be counted at full size and
        // over-claim coverage.
        let painted_box = |node: NodeIndex| -> Rect {
            let mut r = self.tree.bounds(node).intersect(viewport);
            let mut n = node;
            loop {
                let parent = self.tree.parent(n);
                if !parent.is_some() || parent == n {
                    break;
                }
                n = parent;
                if self.meta.get(&n).is_some_and(|m| m.clip) {
                    r = r.intersect(self.tree.bounds(n));
                }
            }
            r
        };
        let mut out = Vec::new();
        for (i, node) in order.iter().enumerate() {
            let Some(m) = self.meta.get(node) else {
                continue;
            };
            if !m.actions.iter().any(|a| matches!(a, Action::Click)) {
                continue;
            }
            let b = self.tree.bounds(*node).intersect(viewport);
            let area = b.width() * b.height();
            if area < 1.0 {
                continue; // W0105 / W0112 own these.
            }
            // Only nodes painted AFTER this one can cover it.
            for later in &order[i + 1..] {
                let Some(lm) = self.meta.get(later) else {
                    continue;
                };
                // Opaque means: a background with full alpha, and not faded.
                let bg = lm
                    .background
                    .or_else(|| self.node_style.get(later).and_then(|st| st.background));
                let opaque_bg = bg.is_some_and(|c| c.a >= 0.999);
                if !opaque_bg || self.effective_opacity(*later) < 0.999 {
                    continue;
                }
                // An ancestor drawing its own background is the node's
                // backdrop, not something covering it.
                if self.is_ancestor_of(*later, *node) {
                    continue;
                }
                let cover = painted_box(*later).intersect(b);
                let covered = (cover.width().max(0.0) * cover.height().max(0.0)) / area;
                if covered < Self::OCCLUSION_COVERAGE {
                    continue;
                }
                let who =
                    m.id.as_ref()
                        .map(|i| format!("`#{}`", i.as_str()))
                        .unwrap_or_else(|| format!("{:?}", m.label));
                let by = lm
                    .id
                    .as_ref()
                    .map(|i| format!("`#{}`", i.as_str()))
                    .unwrap_or_else(|| format!("a {:?}", lm.role));
                let d = lumen_core::Diagnostic::new(
                    lumen_core::codes::W0113,
                    format!(
                        "{who} is interactive but {:.0}% covered by {by}, which \
                         paints over it. It is on screen and enabled, and the \
                         user can neither see nor click it.",
                        covered * 100.0
                    ),
                );
                let d = match self.handle_for_index(node.index()) {
                    Some(h) => d.with_target(h.to_wire(), m.id.as_ref()),
                    None => d,
                };
                out.push(d);
                break; // One finding per hidden node; the first cover explains it.
            }
        }
        out
    }

    /// Whether `maybe_ancestor` is an ancestor of `node`.
    fn is_ancestor_of(&self, maybe_ancestor: NodeIndex, node: NodeIndex) -> bool {
        let mut n = node;
        loop {
            let parent = self.tree.parent(n);
            if !parent.is_some() || parent == n {
                return false;
            }
            n = parent;
            if n == maybe_ancestor {
                return true;
            }
        }
    }

    /// Below this many nodes a frame with no area is not obviously wrong — a
    /// splash screen or a deliberately empty state is a legitimate design.
    const BLANK_FRAME_MIN_NODES: usize = 3;

    /// W0114: the whole frame paints nothing.
    ///
    /// A *whole-frame* fact, which is exactly why no per-node check finds it.
    /// Every individual zero-area node is defensible — `W0105` deliberately
    /// fires only on interactive ones, because a decorative spacer with no size
    /// is not a defect — so a screen where *everything* collapsed passes every
    /// per-node lint while showing the user an empty window.
    ///
    /// Deliberately narrow: "no node has any area" rather than "almost the whole
    /// frame is one colour". The pixel test needs a rendered frame and a sampling
    /// policy, and it would fire on legitimate single-colour designs. This
    /// version needs neither and has no false positives. It does not catch a
    /// frame that paints only background-coloured content — for text, that is
    /// `W0303`'s job, which measures contrast against the composited backdrop.
    fn blank_frame_findings(&self) -> Vec<lumen_core::Diagnostic> {
        fn walk(
            n: &lumen_core::semantics::SemanticsNode,
            total: &mut usize,
            with_area: &mut usize,
        ) {
            *total += 1;
            if n.bounds.width() >= 0.5 && n.bounds.height() >= 0.5 {
                *with_area += 1;
            }
            for c in &n.children {
                walk(c, total, with_area);
            }
        }
        let (mut total, mut with_area) = (0usize, 0usize);
        walk(&self.sem_root(), &mut total, &mut with_area);
        if total < Self::BLANK_FRAME_MIN_NODES || with_area > 0 {
            return Vec::new();
        }
        vec![lumen_core::Diagnostic::new(
            lumen_core::codes::W0114,
            format!(
                "the frame is blank: {total} nodes are built and in the semantic \
                 tree, and not one of them was laid out with any area, so the \
                 window shows only its background. Usually a container that \
                 collapsed to zero size, or a root that produced no content."
            ),
        )]
    }

    /// W0112 findings: nodes laid out entirely outside the window.
    ///
    /// Nothing checked the viewport before this. `check_overflow` is
    /// parent-relative, so a node sitting correctly inside a parent that is
    /// itself off-canvas satisfies it — and the root's own escape from the
    /// window was never examined at all.
    ///
    /// In `lint()` rather than `audit.rs` because the window rect is runtime
    /// state, not something a `SemanticsNode` walk can see.
    /// O0.5: how many findings of one code a frame reports before the rest are
    /// summarised.
    ///
    /// A lint pass that walks every node can produce a finding per node, and
    /// several do: a column laid out taller than the window makes *every* row
    /// an offscreen finding, which is the ordinary shape of a long page. On a
    /// 6600-node view that was 6372 diagnostics a frame, each with its own
    /// formatted message — 10 ms of string building, every frame, describing
    /// one fact 6372 times.
    ///
    /// Nobody can act on 6372 of anything. Fifty is well past the point of
    /// diminishing returns for a human or an agent, and the summary keeps the
    /// true total visible so a cap never reads as "only 50 of these exist".
    const MAX_PER_CODE: usize = 50;

    /// The line that replaces the findings a cap suppressed.
    ///
    /// Carries the real count, so the cap is transparent rather than silent —
    /// a truncation the reader cannot see is worse than the flood.
    fn suppressed_note(
        code: &'static str,
        shown: usize,
        suppressed: usize,
        what: &str,
    ) -> lumen_core::Diagnostic {
        lumen_core::Diagnostic::new(
            code,
            format!(
                "{shown} of {} {what} reported; {suppressed} more suppressed. \
                 A finding repeated this many times is one fact about the view, \
                 not {} separate defects — fix the shared cause.",
                shown + suppressed,
                shown + suppressed
            ),
        )
    }

    /// W0404: text shaped at layout time because a container content-sizes.
    ///
    /// Reported as **one** finding with a count, not one per node: it is a
    /// single fact about the view — a container above them needs their widths —
    /// and a thousand copies of it would be the flood O0.5 exists to prevent.
    ///
    /// The threshold exists because a handful is normal and unavoidable: a menu
    /// or a tooltip that hugs its content is *supposed* to measure its children.
    /// It is worth saying only once it is a list.
    fn indefinite_shaping_findings(&self) -> Vec<lumen_core::Diagnostic> {
        /// Below this it is an ordinary shrink-to-fit container, not a cost.
        const NOTEWORTHY: usize = 64;
        if self.shaped_for_indefinite < NOTEWORTHY {
            return Vec::new();
        }
        vec![lumen_core::Diagnostic::new(
            lumen_core::codes::W0404,
            format!(
                "{} text nodes were shaped during layout because a container \
                 above them sizes itself to its content, so their glyph widths \
                 are needed whether or not they are on screen. Give that \
                 container a definite width (`width: 100%` of its parent is \
                 usually what was meant) and their shaping defers to paint, \
                 which only draws what is visible.",
                self.shaped_for_indefinite
            ),
        )]
    }

    fn offscreen_findings(&self, cap: usize) -> Vec<lumen_core::Diagnostic> {
        let viewport = Rect::new(0.0, 0.0, self.size.width, self.size.height);
        let mut out = Vec::new();
        // O0.5: keep counting past the cap — the predicate is cheap, the
        // `format!` and handle lookup are not — so the summary stays accurate
        // while the cost stops growing.
        let mut suppressed = 0usize;
        for (node, m) in self.meta.iter() {
            // Same "claims to be for something" filter as W0111: a decorative
            // spacer parked off-canvas is not a defect worth interrupting for.
            let interactive = m.actions.iter().any(|a| matches!(a, Action::Click));
            if !interactive && m.label.trim().is_empty() {
                continue;
            }
            let b = self.tree.bounds(*node);
            if b.width() < 0.5 || b.height() < 0.5 {
                continue; // W0105 covers zero-area, and more precisely.
            }
            // Scrolled out of view is what a scroll container is FOR. The
            // container's own `ScrollInfo` is how to reason about those.
            if self.is_in_scroll(*node) {
                continue;
            }
            let overlaps = b.x0 < viewport.x1
                && b.x1 > viewport.x0
                && b.y0 < viewport.y1
                && b.y1 > viewport.y0;
            if overlaps {
                continue;
            }
            if out.len() >= cap {
                suppressed += 1;
                continue;
            }
            let who =
                m.id.as_ref()
                    .map(|i| format!("`#{}`", i.as_str()))
                    .unwrap_or_else(|| format!("{:?}", m.label));
            let d = lumen_core::Diagnostic::new(
                lumen_core::codes::W0112,
                format!(
                    "{who} is laid out at ({:.0}, {:.0}) {:.0}×{:.0}, entirely \
                     outside the {:.0}×{:.0} window. It is built, laid out and \
                     in the semantic tree, and no part of it is on screen.",
                    b.x0,
                    b.y0,
                    b.width(),
                    b.height(),
                    self.size.width,
                    self.size.height
                ),
            );
            let d = match self.handle_for_index(node.index()) {
                Some(h) => d.with_target(h.to_wire(), m.id.as_ref()),
                None => d,
            };
            out.push(d);
        }
        out.sort_by(|a, b| a.message.cmp(&b.message));
        if suppressed > 0 {
            out.push(Self::suppressed_note(
                lumen_core::codes::W0112,
                out.len(),
                suppressed,
                "nodes laid out entirely offscreen",
            ));
        }
        out
    }

    /// Whether `node` sits inside a scroll container (itself included).
    fn is_in_scroll(&self, node: NodeIndex) -> bool {
        let mut n = node;
        loop {
            if self.meta.get(&n).is_some_and(|m| m.scroll().is_some()) {
                return true;
            }
            let parent = self.tree.parent(n);
            if !parent.is_some() || parent == n {
                return false;
            }
            n = parent;
        }
    }

    /// O2.1: the effective opacity of the node a `selector` resolves to, or
    /// `None` if it doesn't resolve to exactly one node.
    ///
    /// Surfaced through `ui.getLayout` beside `ink` and `text_metrics` — the
    /// other per-node visual facts that are deliberately absent from the tree,
    /// which stays lean and carries structure.
    pub fn node_opacity(&self, selector: &str) -> Option<f64> {
        let root = self.semantics_elided();
        let id = lumen_core::semantics::resolve_one(&root, selector).ok()?;
        let node = self.node_for_handle(id)?;
        Some(self.effective_opacity(node) as f64)
    }

    /// Effective opacity below which a node is invisible rather than merely
    /// faint. Not 0.0: an interrupted fade that stopped at 0.004 is as
    /// invisible as one that stopped at 0.0, and `f32` arithmetic over a chain
    /// of layer multiplications will not land on exactly zero.
    const INVISIBLE_OPACITY: f32 = 0.01;

    /// W0111 findings for the current frame (see [`Headless::lint`]).
    ///
    /// Lives here rather than in `audit.rs` for the same reason W0110 and
    /// W0402 do: it needs style data keyed by `NodeIndex`, and a
    /// `SemanticsNode` walk cannot see it.
    fn invisible_findings(&self, cap: usize) -> Vec<lumen_core::Diagnostic> {
        let mut out = Vec::new();
        // O0.5: see MAX_PER_CODE — count past the cap, stop formatting.
        let mut suppressed = 0usize;
        for (node, m) in self.meta.iter() {
            // Decorative fades must stay quiet, or the check gets ignored. Only
            // a node that claims to be *for* something is worth reporting:
            // it is interactive, or it carries a name a user is meant to read.
            let interactive = m.actions.iter().any(|a| matches!(a, Action::Click));
            if !interactive && m.label.trim().is_empty() {
                continue;
            }
            let b = self.tree.bounds(*node);
            if b.width() < 0.5 || b.height() < 0.5 {
                continue; // W0105's business, and it says it better.
            }
            // A fade-in passing through zero on its first frame is not a
            // defect. A fade that *stopped* at zero is — and O3.3's
            // stuck-animation check is what catches that one.
            if let Some(id) = &m.id {
                if self.prop_anims.contains_key(&(id.clone(), "opacity"))
                    || self.key_anims.contains_key(id)
                {
                    continue;
                }
            }
            let eff = self.effective_opacity(*node);
            if eff > Self::INVISIBLE_OPACITY {
                continue;
            }
            let own = self
                .node_style
                .get(node)
                .and_then(|s| s.opacity)
                .unwrap_or(1.0);
            let who =
                m.id.as_ref()
                    .map(|i| format!("`#{}`", i.as_str()))
                    .unwrap_or_else(|| {
                        if m.label.trim().is_empty() {
                            format!("a {:?}", m.role)
                        } else {
                            format!("{:?}", m.label)
                        }
                    });
            // Naming the inherited case explicitly: "my opacity is 1, why is
            // this firing" is the first thing the author will ask.
            let cause = if own > Self::INVISIBLE_OPACITY {
                format!(
                    " — its own opacity is {own:.2}, but an enclosing group \
                     multiplies it to {eff:.3}"
                )
            } else {
                String::new()
            };
            if out.len() >= cap {
                suppressed += 1;
                continue;
            }
            let d = lumen_core::Diagnostic::new(
                lumen_core::codes::W0111,
                format!(
                    "{who} is laid out {:.0}×{:.0} and fully transparent \
                     (effective opacity {eff:.3}){cause}. It occupies space and \
                     answers the semantic tree, but nothing of it is on screen.",
                    b.width(),
                    b.height()
                ),
            );
            let d = match self.handle_for_index(node.index()) {
                Some(h) => d.with_target(h.to_wire(), m.id.as_ref()),
                None => d,
            };
            out.push(d);
        }
        // `self.meta` is a HashMap, so iteration order is unspecified —
        // without this the finding order would churn between runs.
        out.sort_by(|a, b| a.message.cmp(&b.message));
        if suppressed > 0 {
            out.push(Self::suppressed_note(
                lumen_core::codes::W0111,
                out.len(),
                suppressed,
                "invisible nodes",
            ));
        }
        out
    }

    /// APCA lightness contrast below which text is unreadable rather than
    /// merely low-contrast. `ContrastLevel::Fail` starts at 45 — far too eager
    /// for a hard diagnostic, since plenty of legitimate secondary text lives
    /// in the 30s. This is the "you cannot see it" line.
    const LEGIBILITY_FLOOR: f64 = 15.0;

    /// W0303 findings for the current frame (see [`Headless::lint`]).
    ///
    /// Split out rather than inlined because it is the one lint check that
    /// needs a *display list* — `resolve_backdrop` composites the fill stack
    /// under each glyph run — so it is markedly more expensive than the
    /// semantics walks, and the per-frame ambient audit needs to be able to
    /// schedule it separately.
    fn contrast_findings(&mut self) -> Vec<lumen_core::Diagnostic> {
        let report = self.contrast_report();
        report
            .targets
            .iter()
            .filter(|t| t.apca_lc.abs() < Self::LEGIBILITY_FLOOR)
            .map(|t| {
                let who = t
                    .label
                    .as_deref()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| format!("{l:?}"))
                    .or_else(|| t.node.clone().map(|n| format!("`{n}`")))
                    .unwrap_or_else(|| "text".to_string());
                // O0.1b resolved the identity mismatch this originally worked
                // around: `Diagnostic.node` is the author's `StableId` and
                // `TargetContrast.node` is the agent wire handle, so the
                // handle now goes in `Diagnostic.handle`, which exists for
                // exactly this — an always-available, path-derived anchor.
                let d = lumen_core::Diagnostic::new(
                    lumen_core::codes::W0303,
                    format!(
                        "{who}{} is unreadable: APCA Lc {:.1} against its \
                         composited backdrop (foreground {}, background {}). \
                         Below |Lc| {:.0} text is invisible rather than merely \
                         low-contrast.",
                        t.node
                            .as_deref()
                            .map(|n| format!(" ({n})"))
                            .unwrap_or_default(),
                        t.apca_lc,
                        t.foreground,
                        t.background,
                        Self::LEGIBILITY_FLOOR
                    ),
                );
                match &t.node {
                    Some(h) => d.with_handle(h.clone()),
                    None => d,
                }
            })
            .collect()
    }

    // --- desktop system integration (T5.2) ---------------------------------

    /// Read the (in-memory) clipboard text. Backed by the shared `Runtime`
    /// clipboard, so text widgets and this accessor see the same buffer.
    pub fn clipboard_read(&self) -> String {
        self.rt.clipboard()
    }

    /// Write text to the clipboard.
    pub fn clipboard_write(&mut self, text: impl Into<String>) {
        self.rt.set_clipboard(text);
    }

    /// Install the app's native menu model.
    pub fn set_menu(&mut self, menu: crate::system::MenuModel) {
        self.menu = menu;
        self.menu_rev += 1;
    }

    /// The current menu model.
    pub fn menu(&self) -> &crate::system::MenuModel {
        &self.menu
    }

    /// Bumped on every [`set_menu`](Self::set_menu) — the shell rebuilds the
    /// native (muda) menu only when this changes (P.3c).
    pub fn menu_rev(&self) -> u64 {
        self.menu_rev
    }

    /// Invoke a menu command by id; returns its label if it exists and is
    /// enabled, recording the invocation for the app/agent.
    pub fn invoke_menu(&mut self, id: &str) -> Option<String> {
        let label = self
            .menu
            .find(id)
            .filter(|i| i.enabled)
            .map(|i| i.label.clone())?;
        self.invoked_menu.push(id.to_string());
        Some(label)
    }

    /// Menu command ids invoked so far.
    pub fn invoked_menu(&self) -> &[String] {
        &self.invoked_menu
    }

    /// P.3c: a native activation (muda click, accelerator chord, agent
    /// `menu.invoke`) — records the invocation like
    /// [`invoke_menu`](Self::invoke_menu), then *runs* the app command
    /// registered under the same id (`cx.register_command`), so menu items
    /// bound to commands actually drive the app. Pumps either way; returns
    /// the item's label, or `None` if the id is unknown/disabled.
    pub fn activate_menu(&mut self, id: &str) -> Option<String> {
        let label = self.invoke_menu(id)?;
        if self.commands.contains_key(id) {
            let _ = self.run_command(id); // runs the handler + pumps
        } else {
            self.pump();
        }
        Some(label)
    }

    /// Record a request to an OS service (the real shell fulfils it).
    pub fn request_system(&mut self, req: crate::system::SystemRequest) {
        self.system_requests.push(req);
    }

    /// System requests recorded this session.
    pub fn system_requests(&self) -> &[crate::system::SystemRequest] {
        &self.system_requests
    }

    /// P.3d: realize a declared secondary window (`App::window`) as its own
    /// `Headless` instance **sharing this app's `Runtime`** — the same
    /// signal store, deferred-op channel, clipboard, and host mailbox. Each
    /// window has its own tree/layout/paint pipeline at its declared size;
    /// cross-window reactivity is just shared signals (pump a window after
    /// state changes to re-render it). The caller supplies the window's
    /// renderer + executor (the shell passes its per-window backend).
    pub fn open_window_with<R2: lumen_render::Renderer, E2: lumen_core::tasks::Spawner>(
        &self,
        id: &str,
        renderer: R2,
        executor: E2,
    ) -> Option<Headless<R2, E2, P>> {
        let (desc, root) = self.window_decls.iter().find(|(d, _)| d.id == id)?;
        let root = root.clone();
        // Secondary windows share the parent's platform bundle: a window that
        // laid out with a different engine than its owner would produce
        // geometry the owner cannot reason about.
        let app: App<R2, E2, P> = App {
            _platform: std::marker::PhantomData,
            root: Box::new(move |cx| Box::new(Some(root(cx))) as Box<dyn DirectDyn>),
            stylesheet: self.stylesheet_src.clone(),
            fonts: self.font_bytes.clone(),
            windows: Vec::new(),
            renderer,
            executor,
        };
        let mut h = app.into_headless(Size::new(desc.width, desc.height), None);
        // Swap in the shared store before the first build ever runs.
        h.rt = self.rt.clone();
        h.theme = self.theme;
        h.rtl = self.rtl;
        h.rebuild();
        Some(h)
    }

    /// [`open_window_with`](Self::open_window_with) on the default CPU
    /// renderer + inline executor (tests and headless agents).
    pub fn open_window(
        &self,
        id: &str,
    ) -> Option<Headless<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner, P>> {
        self.open_window_with(id, lumen_render::TinySkia, lumen_core::tasks::InlineSpawner)
    }

    /// P.3b: drain the recorded [`SystemRequest`]s — the shell takes them to
    /// fulfil natively (dialogs, notifications); tests/agents that only
    /// observe use [`system_requests`](Self::system_requests).
    pub fn take_system_requests(&mut self) -> Vec<crate::system::SystemRequest> {
        std::mem::take(&mut self.system_requests)
    }

    /// Declare the app's secondary windows (multi-window).
    pub fn set_windows(&mut self, windows: Vec<crate::system::WindowDesc>) {
        self.windows = windows;
    }

    /// The app's secondary windows.
    pub fn windows(&self) -> &[crate::system::WindowDesc] {
        &self.windows
    }

    /// Set the layout direction (T5.3). `true` mirrors the layout right-to-left
    /// for RTL locales; re-lays-out immediately.
    pub fn set_rtl(&mut self, rtl: bool) {
        self.rtl = rtl;
        self.rebuild();
    }

    /// Whether the layout is mirrored right-to-left.
    pub fn is_rtl(&self) -> bool {
        self.rtl
    }

    /// ID1: map an agent-visible [`NodeHandle`] back to a live arena node.
    ///
    /// Selectors now resolve to handles (structural identity), while the
    /// tree/style/layout maps are keyed by `NodeIndex` (arena slot). The
    /// semantic tree carries both, so it is the translation table — and it
    /// stays correct when the arena starts recycling slots, which a direct
    /// index comparison would not.
    fn node_for_handle(&self, handle: lumen_core::identity::NodeHandle) -> Option<NodeIndex> {
        fn walk(
            n: &lumen_core::semantics::SemanticsNode,
            want: lumen_core::identity::NodeHandle,
        ) -> Option<u32> {
            if n.node == want {
                return Some(n.index);
            }
            n.children.iter().find_map(|c| walk(c, want))
        }
        let idx = walk(&self.sem_root(), handle)?;
        self.tree
            .document_order()
            .into_iter()
            .find(|n| n.index() == idx)
    }

    /// The semantics tree, building it if this frame has not needed one yet
    /// (OB2). Returns an `Rc` rather than a borrow so the interior-mutable cache
    /// does not leak a `RefCell` guard into callers — same reasoning as
    /// [`semantics_elided`](Self::semantics_elided).
    fn sem_root(&self) -> Rc<SemanticsNode> {
        if let Some(r) = self.sem_root.borrow().as_ref() {
            return Rc::clone(r);
        }
        let built = Rc::new(self.build_semantics(self.tree.root()));
        *self.sem_root.borrow_mut() = Some(Rc::clone(&built));
        built
    }

    /// The elided semantics root, computed once per rebuild and shared (OB4).
    ///
    /// Prefer this over `semantics_doc().root.elided()`, which deep-clones the
    /// tree twice on every call. Returns an `Rc` rather than a borrow so the
    /// interior-mutable cache doesn't leak a `RefCell` guard into callers.
    pub fn semantics_elided(&self) -> Rc<lumen_core::semantics::SemanticsNode> {
        if let Some(cached) = self.elided_cache.borrow().as_ref() {
            return Rc::clone(cached);
        }
        let built = Rc::new(self.sem_root().elided());
        *self.elided_cache.borrow_mut() = Some(Rc::clone(&built));
        built
    }

    /// Record what this pump did (F4.3), if anything will ever read it.
    ///
    /// `nodes` is a closure so the `Vec` is not built in a lean build: the only
    /// reader is `last_change()`, which is `#[cfg(feature = "snapshot")]`, so
    /// without this a lean build allocated a `Vec<u32>` every pump for a value
    /// nothing could observe.
    #[inline]
    fn record_change(&mut self, kind: &'static str, nodes: impl FnOnce() -> Vec<u32>) {
        #[cfg(feature = "snapshot")]
        {
            self.last_change = ChangeReport {
                kind,
                nodes: nodes(),
            };
        }
        #[cfg(not(feature = "snapshot"))]
        {
            let _ = (kind, nodes);
        }
    }

    /// Drop the memoized elided tree — call wherever `sem_root` is reassigned.
    fn invalidate_semantics_cache(&self) {
        // O0.3: the semantic tree is what every lint finding derives from, so
        // its generation is exactly the signal for "the findings may have
        // changed". Bumped here rather than in `pump` because the tree is also
        // replaced OUTSIDE a pump — `set_stylesheet`, `set_theme` and
        // `resize` all rebuild directly — and those are precisely the edits a
        // developer makes while an agent is watching.
        #[cfg(feature = "dev-observability")]
        self.sem_gen.set(self.sem_gen.get() + 1);
        self.elided_cache.borrow_mut().take();
        // O0.4: derived from the tree, so it dies with it.
        self.handle_index.borrow_mut().take();
        #[cfg(feature = "snapshot")]
        {
            *self.json_cache.borrow_mut() = [None, None];
        }
    }

    /// `semantics_doc().to_json(raw)`, memoized for the current frame.
    ///
    /// Prefer this over calling `to_json` directly: that path deep-clones the
    /// tree and re-serializes it from scratch every call.
    #[cfg(feature = "snapshot")]
    pub fn semantics_json_cached(&self, raw: bool) -> Rc<serde_json::Value> {
        let slot = raw as usize;
        if let Some(v) = self.json_cache.borrow()[slot].as_ref() {
            return Rc::clone(v);
        }
        let built = Rc::new(self.semantics_doc().to_json(raw));
        self.json_cache.borrow_mut()[slot] = Some(Rc::clone(&built));
        built
    }

    /// The semantics document (typed).
    pub fn semantics_doc(&self) -> SemanticsDoc {
        // ID1: report the focused node by handle. The arena index is looked up
        // through the semantic tree rather than emitted directly, so this field
        // survives slot recycling like every other id does now.
        let focused = self.focused_node().and_then(|n| {
            fn find(
                s: &lumen_core::semantics::SemanticsNode,
                idx: u32,
            ) -> Option<lumen_core::identity::NodeHandle> {
                if s.index == idx {
                    return Some(s.node);
                }
                s.children.iter().find_map(|c| find(c, idx))
            }
            find(&self.sem_root(), n.index())
        });
        let root = (*self.sem_root()).clone();
        SemanticsDoc {
            window: WindowInfo {
                width: self.size.width,
                height: self.size.height,
                scale: 1.0,
                focused,
            },
            root,
        }
    }

    // --- event routing ------------------------------------------------------

    fn route(&mut self, ev: Event) {
        // W.0 (ADR-W1): a custom leaf at the event's target gets first
        // refusal — pointer events at the hit-test target, key/text at the
        // focused node. `Handled` consumes the event: no Element-level
        // handlers, no default routing.
        if let Some(node) = self.leaf_event_target(&ev) {
            if let Some(m) = self.meta.get(&node) {
                if let NodeContent::Custom(w) = &m.content {
                    let w = w.clone();
                    let bounds = self.tree.bounds(node);
                    if matches!(
                        w.event(&ev, bounds, &self.rt),
                        lumen_core::events::EventStatus::Handled
                    ) {
                        return;
                    }
                }
            }
        }
        match ev {
            Event::PointerDown(pe) => {
                // A new touch stops the coast, the way it does everywhere else.
                self.fling = None;
                self.pan_vel = (0.0, 0.0, self.clock_ms);
                // Arm touch panning: a finger drag over a scroll container
                // scrolls it, which is the only way to scroll on a phone — the
                // Android shell emits pointer events and never a wheel, so
                // before this a list could only be moved by its scrollbar.
                //
                // Skipped when the press lands on a drag handler (slider,
                // scrollbar thumb): that widget owns the gesture.
                self.pan = None;
                // A new press supersedes any press still waiting for its
                // release (a lost pointer-up, a second finger).
                self.pending_click = None;
                if pe.pointer == lumen_core::events::PointerKind::Touch {
                    let mut n = self.tree.hit_test(pe.pos);
                    while let Some(node) = n {
                        if let Some(m) = self.meta.get(&node) {
                            if m.on_drag().is_some() {
                                break;
                            }
                            if m.on_wheel().is_some() && wheel_can_take(m.scroll().copied()) {
                                self.pan = Some((node, pe.pos));
                                break;
                            }
                        }
                        let parent = self.tree.parent(node);
                        n = parent.is_some().then_some(parent);
                    }
                }
                // Bubble from the hit target up its ancestors, firing the
                // nearest focus/click/drag handlers (decorative children let
                // their interactive ancestor handle the press).
                let mut n = self.tree.hit_test(pe.pos);
                let (mut did_focus, mut did_click, mut did_drag) = (false, false, false);
                let mut caret_hit = None;
                while let Some(node) = n {
                    if let Some(m) = self.meta.get(&node) {
                        if !did_focus && m.focusable {
                            self.focused_id = m.id.clone();
                            did_focus = true;
                        }
                        // Recorded, not fired: the click belongs to the release
                        // (see `pending_click`). Still resolved here, because
                        // the *press* is what picks the target — a release that
                        // lands on a different node must not activate it.
                        if !did_click && m.on_click.is_some() {
                            self.pending_click = Some((node, m.id.clone(), pe.pos));
                            did_click = true;
                        }
                        if !did_drag && m.on_drag().is_some() {
                            self.pressed = Some((node, m.id.clone()));
                            self.apply_drag(node, pe.pos);
                            did_drag = true;
                        }
                        // A text editor places its caret at the press and keeps
                        // `pressed` so a drag extends the selection.
                        if caret_hit.is_none() && m.on_caret_set().is_some() {
                            self.pressed = Some((node, m.id.clone()));
                            caret_hit = Some(node);
                        }
                    }
                    if did_focus && did_click && did_drag {
                        break;
                    }
                    let p = self.tree.parent(node);
                    n = p.is_some().then_some(p);
                }
                if let Some(node) = caret_hit {
                    self.place_caret(node, pe.pos, false);
                }
                // O4.1: a press that reached the root without finding a single
                // handler. `input.click` reports `{"ok": true}` whenever the
                // SELECTOR resolved, regardless of whether anything was hit, so
                // "I clicked it and nothing happened, and the tool said ok" had
                // no trace anywhere. The routing walk already computes this and
                // was throwing it away.
                //
                // Deliberately not a change to `input.click`'s return shape:
                // agents and exported tests depend on it. The information goes
                // to the ring instead.
                #[cfg(feature = "dev-observability")]
                if !did_focus && !did_click && !did_drag && caret_hit.is_none() {
                    let target = self
                        .tree
                        .hit_test(pe.pos)
                        .and_then(|t| self.meta.get(&t))
                        .map(|m| match &m.id {
                            Some(i) => format!("`#{}`", i.as_str()),
                            None => format!("a {:?}", m.role),
                        })
                        .unwrap_or_else(|| "nothing".to_string());
                    self.rt.log(
                        "warn",
                        format!(
                            "pointer press at ({:.0}, {:.0}) hit {target} and \
                             bubbled to the root without reaching any click, \
                             focus or drag handler — nothing will happen",
                            pe.pos.x, pe.pos.y
                        ),
                    );
                }
                // Light dismiss: any element with an `on_dismiss` whose bounds do
                // not contain the press is dismissed (click-away for dropdowns/
                // popovers/menus). The opening press never self-dismisses: the
                // overlay is built on the *next* rebuild, so it isn't in this
                // frame's tree yet.
                self.dismiss_outside(pe.pos);
            }
            Event::PointerUp(pe) => {
                self.pressed = None;
                // A cancelled release (the platform took the gesture — see
                // `PointerEvent::click_count`) ends the press and nothing else:
                // no click, and no fling either, since the user never chose to
                // let go.
                let cancelled = pe.click_count == 0;
                // The click fires here, and only if the release lands back on
                // the node the press picked. A finger that pressed a row and
                // then scrolled has already dropped `pending_click` on slop;
                // this second check catches the release that simply drifted
                // onto a neighbour.
                if let Some((node, id, _)) = self.pending_click.take().filter(|_| !cancelled) {
                    if let Some(t) = self.click_target_at(pe.pos) {
                        let same = match &id {
                            Some(i) => self.meta.get(&t).and_then(|m| m.id.as_ref()) == Some(i),
                            None => t == node,
                        };
                        if same {
                            if let Some(h) = self.meta.get(&t).and_then(|m| m.on_click.clone()) {
                                h(&self.rt);
                            }
                        }
                    }
                }
                self.fling_ms = self.clock_ms;
                if let Some((_, last)) = self.pan.take() {
                    // Coast only from a real flick. Below this the finger was
                    // placed, not thrown, and momentum would feel like drift.
                    const MIN_FLING_PX_S: f64 = 60.0;
                    let (vx, vy, _) = self.pan_vel;
                    if !cancelled && vx.hypot(vy) >= MIN_FLING_PX_S {
                        self.fling = Some((last, vx, vy));
                    }
                }
                self.pan_vel = (0.0, 0.0, self.clock_ms);
                let _ = pe;
            }
            Event::TextInput(te) => {
                if let Some(node) = self.focused_node() {
                    if let Some(h) = self.meta.get(&node).and_then(|m| m.on_text().cloned()) {
                        h(&self.rt, &te.text);
                    }
                }
            }
            Event::PointerMove(pe) => {
                // A finger that travels past the slop is scrolling, not tapping,
                // so the press stops being a candidate click. Latched: the
                // cancel is permanent for this gesture even if the finger comes
                // back to where it started.
                //
                // TOUCH ONLY. A mouse cannot pan (see `pan`), so there is no
                // competing gesture to disambiguate, and cancelling on movement
                // would break the ordinary "press a big button, wiggle, release"
                // that every desktop toolkit activates.
                if pe.pointer == lumen_core::events::PointerKind::Touch {
                    if let Some((_, _, origin)) = self.pending_click {
                        if (pe.pos - origin).hypot() > TOUCH_SLOP_PX {
                            self.pending_click = None;
                        }
                    }
                }
                // Touch pan: content follows the finger, so dragging UP scrolls
                // toward the end — the same sign the shell gives the wheel.
                // Deltas are per-move rather than from the press origin, which
                // is what lets them accumulate through the wheel path unchanged.
                if let Some((_, last)) = self.pan {
                    let (dx, dy) = (last.x - pe.pos.x, last.y - pe.pos.y);
                    if dx.abs() >= 0.5 || dy.abs() >= 0.5 {
                        // Velocity for the release fling. Sampled against the
                        // clock, so several moves inside one frame contribute
                        // displacement without a divide-by-zero; the estimate is
                        // smoothed so one jittery sample cannot launch a coast.
                        let dt = self.clock_ms - self.pan_vel.2;
                        if dt > 0.0 {
                            let (ix, iy) = (dx * 1000.0 / dt, dy * 1000.0 / dt);
                            let a = 0.6;
                            self.pan_vel = (
                                self.pan_vel.0 * (1.0 - a) + ix * a,
                                self.pan_vel.1 * (1.0 - a) + iy * a,
                                self.clock_ms,
                            );
                        }
                        self.pan = self.pan.map(|(n, _)| (n, pe.pos));
                        self.dispatch_wheel(pe.pos, dx, dy, pe.modifiers);
                    }
                }
                let (_l, _e) = self.pointer.update(&self.tree, pe.pos);
                // Hover bubbles to the nearest ancestor with an id (like clicks
                // bubble to an on_click ancestor), so hovering a button's child
                // label still marks the button itself as hovered.
                let mut n = self.tree.hit_test(pe.pos);
                let mut id = None;
                while let Some(node) = n {
                    if let Some(m) = self.meta.get(&node) {
                        if m.id.is_some() {
                            id = m.id.clone();
                            break;
                        }
                    }
                    let p = self.tree.parent(node);
                    n = p.is_some().then_some(p);
                }
                if self.hovered_id != id {
                    self.hovered_id = id.clone();
                    // Publish hover as state so `BuildCx::is_hovered` is a
                    // tracked read (see its docs): only scopes that read it are
                    // invalidated, so pointer motion keeps its memoized
                    // rebuilds. Written only on change, so idle motion within
                    // one node costs nothing.
                    let s: lumen_core::Signal<String> =
                        self.rt.signal(crate::element::HOVER_SIGNAL, String::new);
                    s.set(
                        &self.rt,
                        id.as_ref()
                            .map(|i| i.as_str().to_string())
                            .unwrap_or_default(),
                    );
                }
                if let Some((idx, drag_id)) = self.pressed.clone() {
                    // Re-resolve by stable id so a rebuild that renumbered nodes
                    // doesn't drag the wrong (or a stale) node; fall back to the
                    // original index.
                    let node = drag_id
                        .as_ref()
                        .and_then(|i| self.node_by_id(i))
                        .unwrap_or(idx);
                    // A pressed text editor extends its selection on drag; other
                    // pressed nodes are sliders/scrollbars (fractional drag).
                    if self
                        .meta
                        .get(&node)
                        .is_some_and(|m| m.on_caret_set().is_some())
                    {
                        self.place_caret(node, pe.pos, true);
                    } else {
                        self.apply_drag(node, pe.pos);
                    }
                }
            }
            Event::Wheel(we) => {
                self.dispatch_wheel(we.pos, we.delta.x, we.delta.y, we.modifiers);
            }
            Event::Drop(de) => {
                // Bubble to the nearest ancestor (incl. target) with a drop handler.
                let mut n = self.tree.hit_test(de.pos);
                while let Some(node) = n {
                    if let Some(h) = self.meta.get(&node).and_then(|m| m.on_drop().cloned()) {
                        h(&self.rt, &de.data);
                        break;
                    }
                    let parent = self.tree.parent(node);
                    n = parent.is_some().then_some(parent);
                }
            }
            Event::KeyDown(ke) => {
                // The focused node's key handler sees every key first (a list
                // handles PageUp/Down/Home/End/arrows); built-in focus/activation
                // keys still apply.
                if let Some(node) = self.focused_node() {
                    // Vertical caret nav needs layout geometry (which visual line),
                    // so the app handles Up/Down for text editors; the widget's
                    // on_key handles the rest (Left/Right/Home/End/edit/clipboard).
                    let vnav = match ke.key {
                        Key::Named(NamedKey::ArrowUp) => Some(true),
                        Key::Named(NamedKey::ArrowDown) => Some(false),
                        _ => None,
                    }
                    .filter(|_| {
                        self.meta
                            .get(&node)
                            .is_some_and(|m| m.on_caret_set().is_some())
                    });
                    if let Some(up) = vnav {
                        let extend = ke.modifiers.contains(lumen_core::events::Modifiers::SHIFT);
                        self.move_caret_vertical(node, up, extend);
                    } else if let Some(h) = self.meta.get(&node).and_then(|m| m.on_key().cloned()) {
                        h(&self.rt, &ke);
                    }
                }
                match ke.key {
                    Key::Named(NamedKey::Tab) => {
                        let forward = !ke.modifiers.contains(lumen_core::events::Modifiers::SHIFT);
                        self.move_focus(forward);
                    }
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.activate_focused();
                    }
                    // Escape light-dismisses every open overlay.
                    Key::Named(NamedKey::Escape) => self.dismiss_all(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Fire `on_dismiss` for every element whose bounds do not contain `pos`
    /// (click-away). Collected first, since a handler mutates state.
    fn dismiss_outside(&self, pos: Point) {
        let hits: Vec<Handler> = self
            .tree
            .document_order()
            .into_iter()
            .filter_map(|n| {
                let h = self.meta.get(&n).and_then(|m| m.on_dismiss().cloned())?;
                (!self.dismiss_owner_contains(n, pos)).then_some(h)
            })
            .collect();
        for h in hits {
            h(&self.rt);
        }
    }

    /// Whether `pos` lands on the overlay `n` **or on the element that owns
    /// it** — its direct parent, which is also the trigger's parent.
    ///
    /// Testing the overlay's own bounds alone made every toggling trigger
    /// un-closable. A press on the trigger is outside the panel, so the press
    /// (`PointerDown`) dismissed the panel, and then the release
    /// (`PointerUp`) ran the trigger's toggle and opened it straight back up:
    /// clicking an open dropdown collapsed and instantly re-expanded it.
    ///
    /// The parent is the right frame because that is the shape every anchored
    /// overlay has — a `Position::Relative` wrapper holding the trigger plus an
    /// absolutely-positioned panel — so the wrapper's box *is* "the trigger",
    /// the panel being out of flow. Only the direct parent: walking further up
    /// would reach a root that contains everything and nothing would ever
    /// dismiss.
    ///
    /// `Sheet`/`Drawer` sit inside a full-window wrapper, so this never
    /// dismisses them from a press — their scrim closes them through its own
    /// `on_click` instead, which correctly leaves a press on the panel alone.
    /// Escape is unaffected: it goes through `dismiss_all`.
    fn dismiss_owner_contains(&self, n: NodeIndex, pos: Point) -> bool {
        if self.tree.bounds(n).contains(pos) {
            return true;
        }
        let parent = self.tree.parent(n);
        parent.is_some() && self.tree.bounds(parent).contains(pos)
    }

    /// Fire every `on_dismiss` (Escape closes all overlays).
    fn dismiss_all(&self) {
        let hits: Vec<Handler> = self
            .tree
            .document_order()
            .into_iter()
            .filter_map(|n| self.meta.get(&n).and_then(|m| m.on_dismiss().cloned()))
            .collect();
        for h in hits {
            h(&self.rt);
        }
    }

    fn focused_node(&self) -> Option<NodeIndex> {
        let id = self.focused_id.as_ref()?;
        self.node_by_id(id)
    }

    /// The current node carrying stable id `id`, if any (survives rebuilds).
    fn node_by_id(&self, id: &StableId) -> Option<NodeIndex> {
        self.tree
            .document_order()
            .into_iter()
            .find(|n| self.meta.get(n).and_then(|m| m.id.as_ref()) == Some(id))
    }

    /// The rendered bounds of the node with stable id `id`, if present. Looked
    /// up by id (not node index), so it survives the rebuilds that renumber
    /// nodes — handy for asserting a layout reflowed after a state change.
    pub fn node_bounds_by_id(&self, id: &str) -> Option<Rect> {
        let id: StableId = id.into();
        self.node_by_id(&id).map(|n| self.tree.bounds(n))
    }

    /// The painted caret rectangle (window-space) for the focused editor `id`, or
    /// `None` if it isn't focused / has no caret. Introspection for asserting an
    /// input's caret stays inside its (clipped) box.
    #[doc(hidden)]
    pub fn caret_rect(&self, id: &str) -> Option<Rect> {
        let id: StableId = id.into();
        self.node_by_id(&id)
            .and_then(|n| self.node_caret.get(&n).copied())
    }

    /// Report every node that declares a semantic [`Action`] it does not
    /// implement (`W0106`, W2).
    ///
    /// `actions` is the contract the agent (`input.invokeAction`) and AccessKit
    /// read to decide what a node can do. Declaring `Increment` without an
    /// `on_increment` means the agent's call fails and a screen-reader user is
    /// offered a control that does nothing — the exact semantics-vs-reality
    /// drift ADR-009 exists to prevent. Run it over a screen in a test to keep
    /// the class from coming back.
    pub fn audit_actions(&self) -> Vec<lumen_core::Diagnostic> {
        let mut out = Vec::new();
        for node in self.tree.document_order() {
            let Some(m) = self.meta.get(&node) else {
                continue;
            };
            let who =
                m.id.as_ref()
                    .map(|i| i.as_str().to_string())
                    .unwrap_or_else(|| {
                        if m.label.is_empty() {
                            format!("{:?} node-{}", m.role, node.index())
                        } else {
                            m.label.clone()
                        }
                    });
            for a in &m.actions {
                let missing = match a {
                    Action::Click => m.on_click.is_none(),
                    Action::Focus => !self.tree.flags(node).contains(NodeFlags::FOCUSABLE),
                    Action::Dismiss => m.on_dismiss().is_none(),
                    Action::Increment => m.on_increment().is_none(),
                    Action::Decrement => m.on_decrement().is_none(),
                    Action::SetValue => m.on_set_value().is_none(),
                    // Not routable yet — declaring them is informational, not a
                    // broken promise, so they are not flagged.
                    Action::Blur | Action::ScrollIntoView | Action::Expand | Action::Collapse => {
                        false
                    }
                };
                if missing {
                    let d = lumen_core::Diagnostic::new(
                        lumen_core::codes::W0106,
                        format!("`{who}` declares action `{a:?}` but implements no handler for it"),
                    );
                    let d = match self.handle_for_index(node.index()) {
                        Some(h) => d.with_target(h.to_wire(), m.id.as_ref()),
                        None => d,
                    };
                    out.push(d);
                }
            }
        }
        out
    }

    /// Push `DISABLED` down the tree and strip input from disabled subtrees
    /// (W1).
    ///
    /// Clearing `HIT_TESTABLE`/`FOCUSABLE` is what makes the state *enforced*
    /// rather than cosmetic: `hit_visit` and `focus_ring` already filter on
    /// those bits, so a disabled control cannot be clicked, hovered, dragged or
    /// tabbed to — and therefore cannot report `Disabled` to the agent while
    /// still responding to input.
    fn propagate_disabled(&mut self, node: NodeIndex, inherited: bool) {
        let mut f = self.tree.flags(node);
        let disabled = inherited || f.contains(NodeFlags::DISABLED);
        if disabled {
            f |= NodeFlags::DISABLED;
            f.remove(NodeFlags::HIT_TESTABLE | NodeFlags::FOCUSABLE);
            self.tree.set_flags(node, f);
            // …and stop *advertising* what it can no longer do. The action list
            // is a contract the agent and assistive tech read: a disabled button
            // that still lists `Focus`, or a read-only field still listing
            // `SetValue`, promises behaviour the node has just had removed —
            // which `audit_actions` (W0106) correctly reports as a defect.
            // Clearing them here, after the flags, keeps the two in step for
            // every widget at once rather than per widget.
            if let Some(m) = self.meta.get_mut(&node) {
                m.actions.retain(|a| !Self::is_interactive_action_impl(a));
            }
        }
        let mut c = self.tree.first_child(node);
        while c.is_some() {
            self.propagate_disabled(c, disabled);
            c = self.tree.next_sibling(c);
        }
    }

    /// Whether this action promises *input* the node must be able to accept.
    ///
    /// `ScrollIntoView` is not one — a disabled control can still be revealed,
    /// and screen readers rely on that to describe it.
    fn is_interactive_action_impl(a: &Action) -> bool {
        matches!(
            a,
            Action::Click
                | Action::Focus
                | Action::Blur
                | Action::SetValue
                | Action::Increment
                | Action::Decrement
                | Action::Expand
                | Action::Collapse
                | Action::Dismiss
        )
    }

    /// Whether `node` is disabled (itself or by an ancestor).
    fn is_disabled(&self, node: NodeIndex) -> bool {
        self.tree.flags(node).contains(NodeFlags::DISABLED)
    }

    /// The node a press at `pos` would activate: the nearest ancestor of the hit
    /// target carrying an `on_click`. Mirrors the bubble in the `PointerDown`
    /// arm, and is what the release re-resolves to in order to confirm the
    /// pointer never left the node it pressed.
    fn click_target_at(&self, pos: Point) -> Option<NodeIndex> {
        let mut n = self.tree.hit_test(pos);
        while let Some(node) = n {
            if self.meta.get(&node).is_some_and(|m| m.on_click.is_some()) {
                return Some(node);
            }
            let p = self.tree.parent(node);
            n = p.is_some().then_some(p);
        }
        None
    }

    fn move_focus(&mut self, forward: bool) {
        let current = self.focused_node();
        if let Some(next) = lumen_core::events::next_focus(&self.tree, current, forward) {
            self.focused_id = self.meta.get(&next).and_then(|m| m.id.clone());
        }
    }

    fn activate_focused(&mut self) {
        if let Some(n) = self.focused_node() {
            // A node can become disabled *while* focused; refuse rather than
            // fire a handler the pointer path would have rejected.
            if self.is_disabled(n) {
                return;
            }
            if let Some(h) = self.meta.get(&n).and_then(|m| m.on_click.clone()) {
                h(&self.rt);
            }
        }
    }

    /// Advance momentum by one frame, if a finger left one behind.
    ///
    /// The coast is emitted as ordinary scroll deltas through
    /// [`dispatch_wheel`](Self::dispatch_wheel), so it inherits chaining and
    /// clamping and cannot desynchronise from a drag. Exponential decay with a
    /// per-second factor, evaluated against the elapsed clock rather than a
    /// frame count, so a slow frame decays by the same amount of *time*.
    fn step_fling(&mut self) {
        let Some((pos, vx, vy)) = self.fling else {
            return;
        };
        let dt = (self.clock_ms - self.fling_ms).max(0.0) / 1000.0;
        self.fling_ms = self.clock_ms;
        if dt <= 0.0 {
            return;
        }
        // Stop below a pixel or so per frame: past that it is invisible motion
        // that would keep requesting frames forever.
        const STOP_PX_S: f64 = 40.0;
        const DECAY_PER_S: f64 = 0.002;
        let (dx, dy) = (vx * dt, vy * dt);
        if dx.abs() >= 0.01 || dy.abs() >= 0.01 {
            self.dispatch_wheel(pos, dx, dy, lumen_core::events::Modifiers::empty());
        }
        let k = DECAY_PER_S.powf(dt);
        let (vx, vy) = (vx * k, vy * k);
        self.fling = (vx.hypot(vy) >= STOP_PX_S).then_some((pos, vx, vy));
    }

    /// Deliver a scroll delta at `pos`, to the nearest ancestor (including the
    /// hit target) whose wheel handler can actually use it.
    ///
    /// `WheelHandler` returns `()`, so a handler cannot report "not consumed" —
    /// but `NodeMeta` carries `ScrollInfo`, so the router decides without a
    /// signature change. Before that, a `VirtualList` whose items fit its
    /// viewport swallowed the wheel and its parent never scrolled.
    ///
    /// Shared with touch panning, which synthesizes deltas from finger motion —
    /// so a finger drag chains exactly like a wheel does, for free.
    fn dispatch_wheel(
        &mut self,
        pos: Point,
        dx: f64,
        dy: f64,
        modifiers: lumen_core::events::Modifiers,
    ) {
        let mut n = self.tree.hit_test(pos);
        while let Some(node) = n {
            let m = self.meta.get(&node);
            if let Some(h) = m.and_then(|m| m.on_wheel().cloned()) {
                if wheel_can_take(m.and_then(|m| m.scroll().copied())) {
                    h(&self.rt, dx, dy, modifiers);
                    break;
                }
            }
            let parent = self.tree.parent(node);
            n = parent.is_some().then_some(parent);
        }
    }

    /// Call a node's drag handler with the pointer's fraction along its width and
    /// height (`frac_x`, `frac_y`). Horizontal controls read `frac_x`, vertical
    /// ones (a scrollbar) read `frac_y`.
    fn apply_drag(&self, node: NodeIndex, pos: Point) {
        let b = self.tree.bounds(node);
        if b.width() <= 0.0 && b.height() <= 0.0 {
            return; // degenerate/stale bounds — skip rather than apply (0, 0)
        }
        let frac_x = if b.width() > 0.0 {
            ((pos.x - b.x0) / b.width()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let frac_y = if b.height() > 0.0 {
            ((pos.y - b.y0) / b.height()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Some(h) = self.meta.get(&node).and_then(|m| m.on_drag().cloned()) {
            h(&self.rt, frac_x, frac_y, pos);
        }
    }

    /// Resolve a pointer position over a text-editor node to a byte offset (via
    /// the text engine's geometry) and call its caret handler. `extend` keeps the
    /// selection anchor (drag-select). No-op for non-editor nodes.
    fn place_caret(&mut self, node: NodeIndex, pos: Point, extend: bool) {
        let b = self.tree.bounds(node);
        let Some((text, ts, wrap, padx, pady, handler)) = self.meta.get(&node).and_then(|m| {
            let h = m.on_caret_set().cloned()?;
            let NodeContent::Text(t, ts) = &m.content else {
                return None;
            };
            Some((t.clone(), ts.clone(), m.wrap_width, m.pad.0, m.pad.1, h))
        }) else {
            return;
        };
        // Content-box-local px: x=0 is before the first glyph (matches the text
        // origin, which is painted at the padded corner).
        let lx = (pos.x - b.x0 - padx) as f32;
        let ly = (pos.y - b.y0 - pady) as f32;
        let block = self.text.layout(&text, ts.clone(), &[], wrap, ts.align);
        let byte = block.hit_to_byte(lx, ly);
        handler(&self.rt, byte, extend);
    }

    /// Move a text-editor caret up/down a visual line (geometry lives here, on the
    /// engine side). Resolves the current caret's x to the line above/below and
    /// calls the caret handler. `extend` keeps the selection anchor (Shift).
    fn move_caret_vertical(&mut self, node: NodeIndex, up: bool, extend: bool) {
        let Some((text, ts, wrap, caret, handler)) = self.meta.get(&node).and_then(|m| {
            let h = m.on_caret_set().cloned()?;
            let c = m.caret_byte()?;
            let NodeContent::Text(t, ts) = &m.content else {
                return None;
            };
            Some((t.clone(), ts.clone(), m.wrap_width, c, h))
        }) else {
            return;
        };
        let block = self.text.layout(&text, ts.clone(), &[], wrap, ts.align);
        let (x, y, h) = block.caret_pos(caret);
        // Probe into the neighbouring line (above the caret top, or below its
        // baseline); hit_to_byte clamps to the nearest cluster on that line.
        let ty = if up { y - h * 0.5 } else { y + h * 1.5 };
        let byte = block.hit_to_byte(x, ty);
        handler(&self.rt, byte, extend);
    }

    // --- rebuild ------------------------------------------------------------

    /// Rebuild, containing any panic in the build/layout/paint so a buggy frame
    /// can't take down the window (C2 / T7.3). On panic the previous good frame
    /// is kept and a structured `E0701` diagnostic is recorded; a clean build
    /// clears it.
    /// Rebuild the whole view from current state, bypassing every incremental
    /// cache — the **coherence oracle** (F0): the tree as a pure function of the
    /// store. Snapshot/restore, the CPU golden, replay determinism, and hot
    /// reload all reduce to this one operation, and the fine-grained work (F1+)
    /// must stay equal to it (`assert_view_coherent`).
    pub fn rebuild_fresh(&mut self) {
        self.clear_view_caches();
        self.rebuild();
    }

    /// Drop all memoized `cx.scope` subtrees so the next build is from scratch.
    /// Centralised here so the oracle (`rebuild_fresh`), hot reload, and
    /// non-signal rebuilds (resize/theme/visual-state) share one invalidation
    /// point — those paths change the frame without a tracked signal write, so
    /// the version-based memo can't see them.
    fn clear_view_caches(&mut self) {
        self.scope_cache.borrow_mut().clear();
        // A.5b: the resolution memo is keyed on (desc, ancestors, container)
        // only — sheet/theme/media changes arrive via force-rebuild, so they
        // invalidate here.
        self.style_memo.clear();
    }

    /// Whether `scope` is still in the view.
    ///
    /// Direct evidence is `scope_live` — the scope announced itself this build.
    /// Failing that, a scope is *still* live if any ancestor took the memo-hit
    /// path: that ancestor's cached subtree embeds this one, so it is on screen
    /// even though its own `cx.scope` call never ran. An ancestor that re-ran
    /// without visiting it, by contrast, really did drop it.
    fn scope_is_live(&self, scope: IdHash) -> bool {
        // The root is not a `cx.scope` and so never announces itself, but it is
        // alive for as long as the app is. It only reaches this test now that
        // task owners are sweep candidates — a task declared at the root has
        // `owner == ROOT_ID` — and treating it as dead would evict every
        // root-owned signal in the app.
        if scope == lumen_core::identity::ROOT_ID {
            return true;
        }
        if self.scope_live.borrow().contains(&scope) {
            return true;
        }
        let skipped = self.scope_skipped.borrow();
        let mut cur = scope;
        // O(depth), and only for scopes that did not announce themselves.
        while let Some(parent) = self.rt.parent_scope(cur) {
            if skipped.contains(&parent) {
                return true;
            }
            cur = parent;
        }
        false
    }

    /// F5 GC: drop cached scope subtrees + scope-local signals whose key was not
    /// accessed this build (a keyed-list item that vanished), and cancel the
    /// background tasks those scopes owned (TC1). Keeps a churning list bounded;
    /// correct because an absent scope isn't in the view, so a fresh rebuild
    /// wouldn't produce it either (coherence preserved).
    fn sweep_dead_scopes(&mut self) {
        // Candidates come from both side tables: a scope that is never cacheable
        // (it reads the clock, or animates) has no `scope_cache` entry at all, so
        // scanning the cache alone would never notice its death — and its tasks
        // would outlive it forever.
        let candidates: std::collections::HashSet<IdHash> = {
            let cache = self.scope_cache.borrow();
            let tasks = self.tasks_table.borrow();
            cache
                .keys()
                .copied()
                .chain(tasks.values().map(|slot| slot.owner))
                .collect()
        };
        let dead: Vec<IdHash> = candidates
            .into_iter()
            .filter(|k| !self.scope_is_live(*k))
            .collect();
        for k in dead {
            self.scope_cache.borrow_mut().remove(&k);
            // Cancel before evicting the signals: the token is what stops an
            // in-flight write from reaching a slot that is about to disappear.
            // Explicit rather than leaving it to `TaskSlot::drop`, because an
            // `AbortHandle` captured in a handler keeps the `Rc` alive past
            // removal. Computed once — `subtree_scopes` walks the whole scope map.
            let doomed = self.rt.subtree_scopes(k);
            self.tasks_table.borrow_mut().retain(|_, slot| {
                let keep = !doomed.contains(&slot.owner);
                if !keep {
                    slot.cancel();
                }
                keep
            });
            // Sheds the scope's own signals *and* any nested scope's (ADR-021:
            // identity is a hash, so this walks recorded ownership rather than
            // matching a key prefix).
            self.rt.evict_scope(k);
        }
    }

    /// Assert the current (possibly incrementally-updated) view equals a fresh
    /// rebuild from the same state — the F0 coherence invariant
    /// `incremental == rebuild_fresh`. Compares the display list (render truth,
    /// `DrawCmd: PartialEq`) and the semantics tree (agent truth, via `Debug`).
    /// Trivially true today (every pump is already a fresh rebuild); it gains
    /// teeth as F1/F2 add memoized/retained subtrees. Intended for tests + CI
    /// over the gallery and examples.
    pub fn assert_view_coherent(&mut self) {
        let dl_before = self.last_dl.as_ref().map(|d| d.cmds.clone());
        let sem_before = Self::sem_fingerprint(&self.sem_root());
        self.rebuild_fresh();
        let dl_after = self.last_dl.as_ref().map(|d| d.cmds.clone());
        let sem_after = Self::sem_fingerprint(&self.sem_root());
        assert!(
            dl_before == dl_after,
            "view incoherent: display list differs from a fresh rebuild"
        );
        assert!(
            sem_before == sem_after,
            "view incoherent: semantics tree differs from a fresh rebuild"
        );
    }

    /// The semantics tree rendered for comparison, with the raw arena slot
    /// masked out.
    ///
    /// F2.2: `SemanticsNode::index` is the node's arena slot, and its own
    /// documentation calls it "NOT an identity … will be recycled once the
    /// arena persists". The arena now does persist: a spliced span keeps the
    /// slots it already had, while a from-scratch rebuild allocates new ones.
    /// Comparing slots would therefore fail on every memo hit while saying
    /// nothing about the view.
    ///
    /// Everything that *is* identity stays in the comparison — including
    /// `SemanticsNode::node`, the `NodeHandle` derived from the node's path
    /// through the tree, which is precisely the field that exists to survive
    /// slot recycling. If a splice put a node in the wrong place, the handle
    /// changes and this still catches it.
    fn sem_fingerprint(root: &lumen_core::semantics::SemanticsNode) -> String {
        let text = format!("{root:?}");
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = rest.find("index: ") {
            out.push_str(&rest[..at + "index: ".len()]);
            rest = &rest[at + "index: ".len()..];
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            out.push('_');
            rest = &rest[digits..];
        }
        out.push_str(rest);
        out
    }

    /// F3.4: re-evaluate the paint-only bindings whose deps changed, patch each
    /// node's background in the retained `meta`, and repaint. R2 damage limits
    /// the raster to exactly the changed region — no rebuild, no relayout, no
    /// scope re-run. The retained tree stays a pure function of the store
    /// (guarded by `assert_view_coherent`).
    /// A.5 restyle-only visual path: hover/focus/pressed flipped but no
    /// signal/clock/forced change. Re-flags the old and new target nodes,
    /// re-resolves `.lss` styles for their subtrees (descendant combinators
    /// like `.card:hovered button` reach below the flipped node), rebuilds
    /// semantics, and repaints the damage. Returns `false` — caller must do
    /// a full rebuild — if any re-resolved style changes a layout- or
    /// typography-affecting property (state layout rules relayout for real).
    fn restyle_visual(&mut self, before: &VisualState) -> bool {
        // The nodes whose interaction state flipped: old + new of each kind.
        let mut ids: Vec<StableId> = Vec::new();
        let mut push = |id: &Option<StableId>| {
            if let Some(id) = id {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        };
        push(&before.0);
        push(&self.hovered_id);
        push(&before.1);
        push(&self.focused_id);
        push(&before.2.as_ref().and_then(|(_, id)| id.clone()));
        push(&self.pressed.as_ref().and_then(|(_, id)| id.clone()));
        let nodes: Vec<NodeIndex> = ids.iter().filter_map(|id| self.node_by_id(id)).collect();

        // Refresh the interaction flags (same rule as build_node/copy_node).
        for &node in &nodes {
            let Some(m) = self.meta.get(&node) else {
                continue;
            };
            let mut flags = self.tree.flags(node);
            flags.remove(NodeFlags::FOCUSED | NodeFlags::HOVERED | NodeFlags::PRESSED);
            if m.id.is_some() && m.id == self.focused_id {
                flags |= NodeFlags::FOCUSED;
            }
            if m.id.is_some() && m.id == self.hovered_id {
                flags |= NodeFlags::HOVERED;
            }
            if m.id.is_some() && self.pressed.as_ref().is_some_and(|(_, id)| *id == m.id) {
                flags |= NodeFlags::PRESSED;
            }
            self.tree.set_flags(node, flags);
        }

        if self.style_env.is_some() {
            // Two-pass: resolve every affected subtree first, commit only if
            // nothing layout-affecting changed (else the caller rebuilds).
            let mut pending: Vec<PendingStyle> = Vec::new();
            for &node in &nodes {
                let mut ancestors = self.ancestor_descs(node);
                if !self.restyle_subtree(node, &mut ancestors, &mut pending) {
                    return false;
                }
            }
            for (node, mut css, resolved) in pending {
                // B.5: state flips (hover) are exactly what transitions
                // animate — run the same retarget/blend on the restyle path.
                // O0.10: same fork gate as the build path — the style says
                // whether either call can do anything.
                let id = self.meta.get(&node).and_then(|m| m.id.clone());
                let wants_transition =
                    !css.transitions.is_empty() || self.clock_ms < self.theme_anim_until;
                let wants_keyframes = css.animation.is_some();
                if wants_transition || wants_keyframes {
                    let owned = std::rc::Rc::make_mut(&mut css);
                    if wants_transition {
                        self.apply_transitions(&id, owned);
                    }
                    if wants_keyframes {
                        self.apply_keyframes(&id, owned);
                    }
                }
                self.node_style.insert(node, css);
                self.node_computed.insert(node, resolved);
            }
        }

        *self.sem_root.borrow_mut() = None;

        self.invalidate_semantics_cache();
        self.last_damage = self.paint();
        self.record_change("restyle", || nodes.iter().map(|n| n.index()).collect());
        true
    }

    /// The [`lumen_style::NodeDesc`] for `node`, from its retained meta and
    /// the *current* interaction ids — the restyle-path equivalent of the
    /// desc `build_node` constructs while lowering.
    fn node_desc(&self, node: NodeIndex) -> Option<lumen_style::NodeDesc> {
        let m = self.meta.get(&node)?;
        let mut states = Vec::new();
        if m.id.is_some() && m.id == self.focused_id {
            states.push("focused".to_string());
            states.push("focus".to_string());
        }
        if m.id.is_some() && m.id == self.hovered_id {
            states.push("hovered".to_string());
            states.push("hover".to_string());
        }
        if m.id.is_some() && self.pressed.as_ref().is_some_and(|(_, id)| *id == m.id) {
            states.push("pressed".to_string());
            states.push("active".to_string());
        }
        states.extend(m.states.iter().map(|s| s.as_str().to_string()));
        Some(lumen_style::NodeDesc {
            id: m.id.as_ref().map(|i| i.as_str().to_string()),
            classes: m.classes.clone(),
            states,
            ty: m.role.as_str().to_string(),
        })
    }

    /// Root-first ancestor descs for `node` (excluding `node` itself).
    fn ancestor_descs(&self, node: NodeIndex) -> Vec<lumen_style::NodeDesc> {
        let mut chain = Vec::new();
        let mut cur = node;
        loop {
            if cur == self.tree.root() {
                break;
            }
            cur = self.tree.parent(cur);
            if let Some(d) = self.node_desc(cur) {
                chain.push(d);
            }
            if cur == self.tree.root() {
                break;
            }
        }
        chain.reverse();
        chain
    }

    /// Resolve styles for `node` and its subtree against `ancestors`
    /// (mutated as the walk descends), collecting results into `pending`.
    /// Returns `false` if a re-resolved style differs in a layout- or
    /// typography-affecting property from the retained one.
    fn restyle_subtree(
        &self,
        node: NodeIndex,
        ancestors: &mut Vec<lumen_style::NodeDesc>,
        pending: &mut Vec<PendingStyle>,
    ) -> bool {
        let Some(env) = &self.style_env else {
            return true;
        };
        let Some(desc) = self.node_desc(node) else {
            return true;
        };
        // Container queries: nearest `.container()` ancestor's current size.
        let media = 'm: {
            for c in self.container_nodes.iter().rev() {
                let mut cur = node;
                while cur != self.tree.root() {
                    cur = self.tree.parent(cur);
                    if cur == *c {
                        let b = self.tree.bounds(*c);
                        break 'm std::borrow::Cow::Owned(lumen_style::MediaContext {
                            container: Some((b.width(), b.height())),
                            ..env.media.clone()
                        });
                    }
                }
            }
            std::borrow::Cow::Borrowed(&env.media)
        };
        let computed = lumen_style::resolve_with_ancestors(&env.sources, &desc, ancestors, &media);
        let mut css = lumen_style::Style::new();
        let mut resolved = HashMap::default();
        for (prop, c) in &computed {
            lumen_style::apply(&mut css, prop, &c.value, &env.tokens);
            resolved.insert(
                prop.clone(),
                lumen_style::Computed {
                    value: lumen_style::resolve_token(&c.value, &env.tokens),
                    important: c.important,
                    origin: c.origin,
                    span: c.span,
                },
            );
        }
        // B.6b: re-apply the node's retained inline style over the fresh
        // sheet resolution (same origin order as build_node).
        if let Some(inline) = self.meta.get(&node).and_then(|m| m.css_inline.as_deref()) {
            merge_inline_style(&mut css, &mut resolved, inline);
        }
        let old = self.node_style.get(&node);
        if layout_affecting_differ(old, &css) {
            return false;
        }
        pending.push((node, std::rc::Rc::new(css), std::rc::Rc::new(resolved)));
        ancestors.push(desc);
        let mut c = self.tree.first_child(node);
        while c.is_some() {
            if !self.restyle_subtree(c, ancestors, pending) {
                ancestors.pop();
                return false;
            }
            c = self.tree.next_sibling(c);
        }
        ancestors.pop();
        true
    }

    /// F3.5: try to satisfy changed **text** bindings without a rebuild.
    ///
    /// Returns `false` — caller must rebuild — if any changed binding would
    /// move layout, or ellipsizes. MUT1: the verdict is per binding. The
    /// patchable ones commit their node content even then — the rebuild that
    /// follows splices their spans and keeps the values, and
    /// `settle_bindings_for_rebuild` skips them as current — while each
    /// decliner's scope chain is evicted so the rebuild re-runs only that. A
    /// half-patched *frame* is still not a state this can produce: on decline
    /// no paint or semantics update happens here, the rebuild does both.
    fn patch_text_bindings(&mut self, stale: &[usize]) -> bool {
        let rt = self.rt.clone();
        if stale.is_empty() {
            return true;
        }
        // Phase 1 — evaluate and measure, committing nothing yet. MUT1: the
        // verdict is per binding. Patchable ones commit below even when a
        // sibling declines — the rebuild that follows splices their spans and
        // keeps the committed values, so the work is not wasted — and one
        // decliner no longer converts a K-binding patch into an O(N) frame.
        let mut pending: Vec<(usize, String, lumen_core::state::ReadSet, f32, f32)> =
            Vec::with_capacity(stale.len());
        let mut declined = false;
        for &i in stale {
            if !self.text_bindings[i].patchable {
                declined = true;
                continue;
            }
            let node = self.text_bindings[i].node;
            let (s, reads) = self.text_bindings[i].dynamic.eval_isolated(&rt);
            if self.text_bindings[i].deferred {
                // A newline would add a line box the deferral guard promised
                // away; the rebuild's own guard then routes the node down the
                // eager path.
                if s.contains('\n') {
                    declined = true;
                    continue;
                }
                let b = &self.text_bindings[i];
                pending.push((i, s, reads, b.w, b.h));
                continue;
            }
            let wrap = self.text_bindings[i].wrap;
            let Some(ts) = self.meta.get(&node).and_then(|m| match &m.content {
                NodeContent::Text(_, ts) => Some(ts.clone()),
                _ => None,
            }) else {
                declined = true;
                continue;
            };
            let block = self.text.shaped(&s, &ts, wrap, ts.align);
            let (w, h) = (block.width().ceil(), block.height().ceil());
            let b = &self.text_bindings[i];
            // Only an axis whose size CAME from the measurement can move.
            if (b.auto_w && w != b.w) || (b.auto_h && h != b.h) {
                declined = true;
                continue;
            }
            pending.push((i, s, reads, w, h));
        }
        // Phase 2 — commit. Every binding above is layout-neutral, so the
        // retained layout is still correct and only paint and semantics change.
        let mut patched: Vec<u32> = Vec::new();
        let mut patched_nodes: Vec<NodeIndex> = Vec::new();
        for (i, s, reads, w, h) in pending {
            let node = self.text_bindings[i].node;
            Self::reindex_binding(
                &mut self.binding_index,
                BindingSlot::Text(i as u32),
                &self.text_bindings[i].deps,
                &reads,
            );
            self.text_bindings[i].deps = reads;
            self.text_bindings[i].w = w;
            self.text_bindings[i].h = h;
            if let Some(m) = self.meta.get_mut(&node) {
                // A conditional binding can change its read set when it
                // switches branches; the observability projection must follow
                // or the patched frame's semantics disagree with a fresh
                // rebuild (caught by `assert_view_coherent` in the
                // switching-signals test — a pre-MUT1 gap).
                #[cfg(feature = "dev-observability")]
                {
                    m.deps.text = self.text_bindings[i].deps.dep_keys(&rt);
                }
                // The string is the node's content *and* its accessible label,
                // exactly as `build_node` keeps them (`Element::text` sets
                // both) — a patched frame that updated only one of the two
                // would drift from what a rebuild produces, which is precisely
                // what `assert_view_coherent` compares.
                m.label = s.clone();
                if let NodeContent::Text(t, _) = &mut m.content {
                    *t = s;
                }
            }
            patched.push(node.index());
            patched_nodes.push(node);
        }
        if declined {
            // MUT1: the decliners fall to `settle_bindings_for_rebuild`, which
            // evicts exactly their scope chains; the pump rebuilds, which also
            // repaints — so no paint or semantics work here.
            return false;
        }
        // The accessible name changed, so the memoized semantics tree is stale.
        // The background patch below does not need this and does not do it;
        // text does.
        *self.sem_root.borrow_mut() = None;
        self.invalidate_semantics_cache();
        // MUT2: rewrite the patched runs in the retained display list; the
        // full rebuild-and-diff paint is the fallback, not the steady state.
        self.last_damage = match self.paint_patched(&patched_nodes, &[]) {
            Some(d) => d,
            None => self.paint(),
        };
        self.last_build_gen = self.rt.write_gen();
        self.record_change("patch", || patched);
        true
    }

    /// F3.6: settle stale bindings *before* a rebuild decides what to splice.
    ///
    /// This is what lets a bound node be spliced at all. A span is only safe to
    /// splice if its retained `meta` is already what a fresh lowering would
    /// produce; a binding whose signal moved since the last build breaks that.
    /// Rather than banning such spans from the splice path — the old `impure`
    /// rule, which cost a full re-lowering of everything around them — the
    /// runtime brings `meta` up to date here, and the splice is then sound by
    /// the same argument as any other node.
    ///
    /// Backgrounds are paint-only, so they always settle. Text settles when the
    /// new string measures the same, exactly as `patch_text_bindings` decides
    /// it. When it does not, the node's box really is changing and no amount of
    /// patching helps: the view caches are dropped so nothing splices and every
    /// binding is re-evaluated by `build_node` against fresh layout.
    ///
    /// MUT1: a binding that cannot settle evicts exactly the chain of cached
    /// scopes whose spans contain its node, so the rebuild re-runs that chain
    /// and splices everything else. This replaced `clear_view_caches()`, which
    /// dropped every span for one bad label — measured as the decline cliff:
    /// 320 ms vs 9.4 ms for an honest one-chunk rebuild at N=50 000.
    fn settle_bindings_for_rebuild(&mut self) {
        let rt = self.rt.clone();
        for i in 0..self.bg_bindings.len() {
            if self.bg_bindings[i].deps.is_current(&rt) {
                continue;
            }
            let (color, reads) = self.bg_bindings[i].dynamic.eval_isolated(&rt);
            let node = self.bg_bindings[i].node;
            self.bg_bindings[i].deps = reads;
            if let Some(m) = self.meta.get_mut(&node) {
                #[cfg(feature = "dev-observability")]
                {
                    m.deps.background = self.bg_bindings[i].deps.dep_keys(&rt);
                }
                m.background = Some(color);
            }
        }
        let mut evict: Vec<NodeIndex> = Vec::new();
        for i in 0..self.text_bindings.len() {
            if self.text_bindings[i].deps.is_current(&rt) {
                continue;
            }
            let node = self.text_bindings[i].node;
            if !self.text_bindings[i].patchable {
                evict.push(node);
                continue;
            }
            let Some(ts) = self.meta.get(&node).and_then(|m| match &m.content {
                NodeContent::Text(_, ts) => Some(ts.clone()),
                _ => None,
            }) else {
                evict.push(node);
                continue;
            };
            let (s, reads) = self.text_bindings[i].dynamic.eval_isolated(&rt);
            if self.text_bindings[i].deferred {
                if s.contains('\n') {
                    evict.push(node);
                    continue;
                }
                self.text_bindings[i].deps = reads;
                if let Some(m) = self.meta.get_mut(&node) {
                    #[cfg(feature = "dev-observability")]
                    {
                        m.deps.text = self.text_bindings[i].deps.dep_keys(&rt);
                    }
                    m.label = s.clone();
                    if let NodeContent::Text(t, _) = &mut m.content {
                        *t = s;
                    }
                }
                continue;
            }
            let wrap = self.text_bindings[i].wrap;
            let block = self.text.shaped(&s, &ts, wrap, ts.align);
            let (w, h) = (block.width().ceil(), block.height().ceil());
            let b = &self.text_bindings[i];
            if (b.auto_w && w != b.w) || (b.auto_h && h != b.h) {
                evict.push(node);
                continue;
            }
            self.text_bindings[i].deps = reads;
            self.text_bindings[i].w = w;
            self.text_bindings[i].h = h;
            if let Some(m) = self.meta.get_mut(&node) {
                #[cfg(feature = "dev-observability")]
                {
                    m.deps.text = self.text_bindings[i].deps.dep_keys(&rt);
                }
                m.label = s.clone();
                if let NodeContent::Text(t, _) = &mut m.content {
                    *t = s;
                }
            }
        }
        if !evict.is_empty() {
            self.evict_scopes_containing(&evict);
        }
    }

    /// MUT1: evict every cached scope whose recorded span contains one of
    /// `nodes`. `scope_spans` still holds the previous build's records here
    /// (settle runs before the `prev_spans` swap), and span roots are exactly
    /// the subtree roots — so "contains" is an ancestor walk. Everything not
    /// on a chain keeps its cache and splices.
    fn evict_scopes_containing(&self, nodes: &[NodeIndex]) {
        let mut root_to_key: HashMap<NodeIndex, Vec<IdHash>> = HashMap::default();
        for (k, r) in &self.scope_spans {
            root_to_key.entry(r.root).or_default().push(*k);
        }
        let mut cache = self.scope_cache.borrow_mut();
        for &n in nodes {
            let mut cur = n;
            while cur.is_some() {
                if let Some(keys) = root_to_key.get(&cur) {
                    for k in keys {
                        cache.remove(k);
                    }
                }
                cur = self.tree.parent(cur);
            }
        }
    }

    /// Whether any retained text binding's dependencies have changed (F3.5).
    /// MUT1: written signals → stale binding indices, via the reverse index.
    /// Sorted and deduplicated; `is_current` stays the authority (a logged
    /// signal may resolve to bindings that were already refreshed), the index
    /// only narrows the scan from O(bindings) to O(writes).
    fn stale_bindings(&self, written: &[SignalId]) -> (Vec<usize>, Vec<usize>) {
        if written.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let mut text: Vec<usize> = Vec::new();
        let mut bg: Vec<usize> = Vec::new();
        for sig in written {
            if let Some(slots) = self.binding_index.get(sig) {
                for s in slots {
                    match *s {
                        BindingSlot::Text(i) => text.push(i as usize),
                        BindingSlot::Bg(i) => bg.push(i as usize),
                    }
                }
            }
        }
        text.sort_unstable();
        text.dedup();
        bg.sort_unstable();
        bg.dedup();
        text.retain(|&i| !self.text_bindings[i].deps.is_current(&self.rt));
        bg.retain(|&i| !self.bg_bindings[i].deps.is_current(&self.rt));
        (text, bg)
    }

    /// MUT1: rebuild the reverse index from the live binding records. Called
    /// once per rebuild, after lowering and the F3.6 carry-forward, so the
    /// index always describes exactly the records the patch path will touch.
    fn rebuild_binding_index(&mut self) {
        self.binding_index.clear();
        for (i, b) in self.text_bindings.iter().enumerate() {
            for id in b.deps.signal_ids() {
                self.binding_index
                    .entry(id)
                    .or_default()
                    .push(BindingSlot::Text(i as u32));
            }
        }
        for (i, b) in self.bg_bindings.iter().enumerate() {
            for id in b.deps.signal_ids() {
                self.binding_index
                    .entry(id)
                    .or_default()
                    .push(BindingSlot::Bg(i as u32));
            }
        }
    }

    /// MUT1: move one binding between index buckets when a patch changed its
    /// read set (a conditional binding reading different signals per branch).
    fn reindex_binding(
        index: &mut HashMap<SignalId, Vec<BindingSlot>>,
        slot: BindingSlot,
        old: &lumen_core::state::ReadSet,
        new: &lumen_core::state::ReadSet,
    ) {
        let old_ids: Vec<SignalId> = old.signal_ids().collect();
        let new_ids: Vec<SignalId> = new.signal_ids().collect();
        if old_ids == new_ids {
            return;
        }
        for id in &old_ids {
            if new_ids.contains(id) {
                continue;
            }
            if let Some(v) = index.get_mut(id) {
                v.retain(|s| *s != slot);
            }
        }
        for id in &new_ids {
            if !old_ids.contains(id) {
                let v = index.entry(*id).or_default();
                if !v.contains(&slot) {
                    v.push(slot);
                }
            }
        }
    }

    fn patch_bg_bindings(&mut self, stale: &[usize]) {
        let rt = self.rt.clone();
        let mut patched: Vec<u32> = Vec::new();
        let mut patched_nodes: Vec<NodeIndex> = Vec::new();
        for &i in stale {
            let (color, reads) = self.bg_bindings[i].dynamic.eval_isolated(&rt);
            let node = self.bg_bindings[i].node;
            Self::reindex_binding(
                &mut self.binding_index,
                BindingSlot::Bg(i as u32),
                &self.bg_bindings[i].deps,
                &reads,
            );
            self.bg_bindings[i].deps = reads;
            if let Some(m) = self.meta.get_mut(&node) {
                #[cfg(feature = "dev-observability")]
                {
                    m.deps.background = self.bg_bindings[i].deps.dep_keys(&rt);
                }
                m.background = Some(color);
            }
            patched.push(node.index());
            patched_nodes.push(node);
        }
        self.last_damage = match self.paint_patched(&[], &patched_nodes) {
            Some(d) => d,
            None => self.paint(),
        };
        self.last_build_gen = self.rt.write_gen();
        self.record_change("patch", || patched);
    }

    fn rebuild(&mut self) {
        // Default to "nothing painted"; a successful paint sets the real damage.
        self.last_damage = Damage::None;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.rebuild_inner()));
        match result {
            Ok(()) => {
                self.build_panic = None;
                // Autofocus: after a good build with no focus placed, the
                // first autofocus node (document order) takes it.
                if self.focused_id.is_none() {
                    let mut stack = vec![self.tree.root()];
                    while let Some(node) = stack.pop() {
                        if !node.is_some() {
                            continue;
                        }
                        if let Some(m) = self.meta.get(&node) {
                            if m.autofocus && m.focusable && m.id.is_some() {
                                self.focused_id = m.id.clone();
                                break;
                            }
                        }
                        // Sibling below child on the stack ⇒ document order.
                        stack.push(self.tree.next_sibling(node));
                        stack.push(self.tree.first_child(node));
                    }
                }
            }
            Err(payload) => {
                let msg = panic_msg(&payload);
                // C.2: panics reach the agent's `app.logs` too, not just
                // `app.diagnostics` — logs survive after the diagnostic clears.
                self.rt.log("error", format!("E0701 build panicked: {msg}"));
                self.build_panic = Some(lumen_core::Diagnostic::new(
                    lumen_core::codes::E0701,
                    format!("build panicked (frame contained): {msg}"),
                ));
            }
        }
        // MUT1: the binding tables are final for this build (lowering pushed
        // fresh records, the carry-forward kept spliced ones) — index them.
        self.rebuild_binding_index();
        // Baseline the skip-rebuild state after every build, so the next pump only
        // rebuilds on a real change (the build itself may bump the write-gen via
        // memo recomputes — capture the post-build value).
        self.force_rebuild = false;
        self.last_build_gen = self.rt.write_gen();
        self.last_build_clock = self.clock_ms;
        // F4.3: a structural rebuild. Per-node change-diffing is deferred; the
        // agent reads the fresh tree via `getTree`. (Patches report exact nodes.)
        self.record_change("rebuild", Vec::new);
    }

    fn rebuild_inner(&mut self) {
        // F3.6: bring bound nodes up to date BEFORE deciding what to splice, so
        // a span carrying a binding is as safe to reuse as any other. Skipped on
        // the container re-pass: that path resets the arena, so nothing splices
        // and every binding is re-evaluated by `build_node` regardless.
        if !self.container_repass {
            self.settle_bindings_for_rebuild();
        }
        // F3.4: capture the root build's reads (structural — a change rebuilds).
        // Scope reads propagate into this window; paint-only bindings evaluated
        // in `build_node` isolate themselves out. F3.5: text-binding reads
        // isolate out too — they patch, and settle above when they cannot.
        let rt = self.rt.clone();
        self.scope_live.borrow_mut().clear();
        self.scope_skipped.borrow_mut().clear();
        let (root_el, mut requests, root_reads) = {
            let mut cx = BuildCx::new(
                &self.rt,
                self.clock_ms,
                &self.scope_cache,
                &self.scope_live,
                &self.scope_skipped,
                &self.tasks_table,
                self.size,
            );
            let (el, reads) = rt.collect_reads(|| (self.root)(&mut cx));
            (el, cx.take_requests(), reads)
        };
        // F5 GC: sweep cached scopes + scope-local signals absent this build.
        self.sweep_dead_scopes();
        // C.4b: last registration of a command name wins; the map is
        // rebuilt per build like handlers.
        self.commands = requests.commands.iter().cloned().collect::<HashMap<_, _>>();
        // P.3c: a build-declared menu (`cx.set_menu`) installs only when the
        // model actually changed, so `menu_rev` (the shell's native-menu
        // rebuild trigger) doesn't churn on every build.
        if let Some(m) = requests.menu.take() {
            if m != self.menu {
                self.set_menu(m);
            }
        }
        self.requests = requests;
        self.structural_reads = root_reads;
        // F3.6: the previous build's binding records are set aside, not
        // dropped. A spliced span's nodes survive the rebuild (F2.2 retains
        // their `NodeIndex`), and so must their bindings — otherwise the only
        // way to keep a bound node correct would be to re-lower it, which is
        // exactly the `impure` rule this replaces. The carry-forward after the
        // free walk keeps the records whose nodes are still alive.
        let prev_bg_bindings = std::mem::take(&mut self.bg_bindings);
        let prev_text_bindings = std::mem::take(&mut self.text_bindings);

        // Dispatch background-work requests this build emitted, on the executor.
        // The runtime owns the executor + the deferred-op channel, so it mints
        // the sink here (the executor never leaked into `BuildCx`). Results flow
        // back through the channel and are applied at the top of the next pump.
        //
        // TC1: the sink is bound to the task's cancel token, and the backend
        // handle is filed in the slot the declaration registered — this is the
        // only place the two halves of cancellation meet, because the token
        // exists from *declare* time while the handle only exists from here on.
        let tasks = std::mem::take(&mut self.requests.tasks);
        for req in tasks {
            // The sweep above may already have cancelled it (a task declared
            // inside a scope that this same build retired). Don't start work
            // that is dead on arrival.
            let slot = self.tasks_table.borrow().get(&req.id).cloned();
            let Some(slot) = slot else { continue };
            if slot.is_cancelled() {
                continue;
            }
            let sink = self
                .rt
                .make_sink_for(self.task_waker.clone(), req.token.clone());
            let handle = match req.kind {
                crate::element::TaskKind::Blocking(job) => {
                    self.executor.spawn_blocking(Box::new(move || job(sink)))
                }
                crate::element::TaskKind::Future(make) => self.executor.spawn(make(sink)),
            };
            slot.attach(handle);
            // An `InlineSpawner` runs the work *inside* `spawn` above, so a task
            // that cancelled itself mid-run would have its handle attached after
            // the fact. Re-check and abort so the slot never holds a live handle
            // for finished-and-cancelled work.
            if slot.is_cancelled() {
                slot.cancel();
            }
        }

        // A.2: styles resolve *before* layout, inline in `build_node`, so
        // `.lss` layout properties reach taffy. Build the cascade env once
        // per rebuild; clear the per-node results (NodeIndex values are
        // generational — a reused index must not inherit a stale style).
        // A.3.2: last build's tree + per-node work become the copy-forward
        // source; this build's maps start empty and are filled by lowering
        // or by moving entries across from `prev_*`.
        //
        // F2.2: the arena and its side tables are RETAINED. Only the span
        // records are rotated — everything else stays keyed by the node
        // indices it was already keyed by, which is the whole point of
        // splice-in-place: a memo-hit span's `NodeMeta`, styles and layout
        // styles never move, so the per-node memmove they used to cost is
        // simply not paid. Entries belonging to nodes this build replaces are
        // dropped by the free walk further down, after the build knows which
        // those are.
        self.prev_spans = std::mem::take(&mut self.scope_spans);
        self.impure_seen = 0;
        self.shaped_for_indefinite = 0;
        self.nodes_rebuilt = 0;
        self.nodes_copied = 0;
        self.desc_stack.clear();
        *self.desc_hash_stack.borrow_mut() = vec![Some(lumen_core::identity::IdHasher::new())];
        self.container_nodes.clear();
        self.container_stack.clear();
        self.hidden_count = 0;
        self.disabled_count = 0;
        self.style_env = self.app_sheet.as_ref().map(|sheet| StyleEnv {
            sources: [lumen_style::StyleSource {
                origin: lumen_style::Origin::App,
                sheet: sheet.clone(),
            }],
            tokens: lumen_style::tokens_for(sheet, self.theme),
            media: lumen_style::MediaContext {
                width: self.size.width,
                height: self.size.height,
                scale: self.scale,
                platform: if cfg!(target_os = "windows") {
                    "windows"
                } else if cfg!(target_os = "macos") {
                    "macos"
                } else if cfg!(target_os = "android") {
                    "android"
                } else if cfg!(target_os = "ios") {
                    "ios"
                } else {
                    "linux"
                }
                .to_string(),
                // Desktop shells synthesize mouse pointers; the mobile
                // shells flip this to "touch" when they wire input (P.1).
                pointer: if cfg!(any(target_os = "android", target_os = "ios")) {
                    "touch"
                } else {
                    "mouse"
                }
                .to_string(),
                // B.2b: per-node — set from `container_stack` at resolve time.
                container: None,
            },
            keyframes: {
                let tokens = lumen_style::tokens_for(sheet, self.theme);
                let mut map = HashMap::default();
                for item in &sheet.items {
                    if let lumen_style::Item::Keyframes(kf) = item {
                        let mut stops: Vec<(f32, KeyStop)> = kf
                            .stops
                            .iter()
                            .map(|(pct, decls)| {
                                let mut scratch = lumen_style::Style::new();
                                for d in decls {
                                    lumen_style::apply(
                                        &mut scratch,
                                        &d.property,
                                        &d.value,
                                        &tokens,
                                    );
                                }
                                (
                                    *pct / 100.0,
                                    KeyStop {
                                        background: scratch.background,
                                        color: scratch.color,
                                        opacity: scratch.opacity,
                                        border_radius: scratch.border_radius,
                                    },
                                )
                            })
                            .collect();
                        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                        map.insert(kf.name.clone(), stops);
                    }
                }
                map
            },
        });

        // R3/R4: size every per-frame container from the PREVIOUS frame's node
        // count. All four are rebuilt from empty each rebuild, so without a
        // hint each grows by doubling and memmoves at every step — 7.9% of a
        // 3000-row frame in taffy's slotmap alone, plus 5.6% in the hashbrown
        // inserts and ~1.1% in the arena (`docs/profile-vs-iced-2026-08-19.md`).
        // The count is a hint, not a contract: a frame that grows simply
        // reallocates once, as before.
        // MOD1: the bundle's layout engine. R6: taken from the retained
        // scratch and cleared rather than constructed, so its capacity
        // survives the frame; put back at every exit below.
        let mut layout = std::mem::take(&mut self.layout_scratch);
        // F2.1: a retaining engine keeps last frame's nodes so memo-hit spans
        // can reuse them (`copy_node`); the nodes that were NOT reused are
        // freed after the build, below. An engine that does not retain keeps
        // the original clear-and-rebuild behaviour exactly.
        // …but only when a splice is possible at all, i.e. the previous build
        // recorded at least one scope span. Retaining costs a per-node free
        // for everything not reused, where a wholesale reset releases the
        // frame in one go; for a view with no `cx.scope` in it that trade is
        // pure loss — measured at **+7.9%** on `build_frame/lumen` (the
        // no-scope bench) before this guard.
        //
        // The predictor is "are there spans", NOT "did the last build splice
        // anything". The latter deadlocks: splicing requires the retained
        // arena, so a splice count of zero would disable the very thing that
        // makes it non-zero, and the count could never rise again.
        //
        // The arena and the layout tree MUST be reset together. Every arena
        // node carries the taffy handle it was laid out with, so clearing one
        // and keeping the other leaves live nodes pointing at freed taffy
        // slots — which taffy reports as a panic inside `new_with_children`
        // the moment a rebuilt container tries to adopt one.
        self.layout_reuse = layout.retains_nodes() && !self.prev_spans.is_empty();
        let mut tree;
        let mut meta;
        if self.layout_reuse {
            // F2.2: take the LIVE tree, not a fresh one. Nodes this build does
            // not touch stay exactly where they are; `build_node` allocates
            // alongside them and the previous frame's spine is freed once the
            // build knows which nodes that is.
            tree = std::mem::replace(&mut self.tree, Tree::new());
            meta = std::mem::take(&mut self.meta);
        } else {
            layout.clear();
            let hint = self.tree.len();
            self.tree = Tree::new();
            tree = Tree::with_capacity(hint);
            meta = std::mem::take(&mut self.meta);
            meta.clear();
            self.node_style.clear();
            self.node_computed.clear();
            // Every span record now names a dead node, so `splice_span` bails
            // and each scope is lowered normally — a mispredicted frame is
            // slower, never wrong.
        }
        // MUT3: stamp this build, so the bounds walk below can tell a spliced
        // node (retained slot, older epoch) from a freshly lowered one that
        // recycled the same index.
        tree.bump_epoch();
        let old_root = tree.root();
        let (root_node, root_lnode) = Sink {
            app: self,
            tree: &mut tree,
            layout: &mut layout,
            meta: &mut meta,
            // The root's containing block is the viewport, which always has a
            // width; nothing stretches the root itself.
            cb_definite: true,
            stretched: false,
        }
        .lower_root(root_el);
        debug_assert_eq!(root_node, tree.root(), "build left the tree root unset");
        // F2.2: free the previous frame's spine — arena node, side-table
        // entries and taffy node together.
        //
        // The doomed set is exactly what is still reachable from the OLD root
        // once every spliced span has been detached out of it. `splice_span`
        // detaches each span root from its previous parent as it moves it, so
        // walking down from `old_root` never crosses into a surviving span and
        // the walk is O(dead), not O(tree). The check against the *current*
        // root covers the one case detaching cannot: a view whose root element
        // is itself a memo hit, where the old root and the new root are the
        // same node.
        //
        // ORDER MATTERS, and not for the reason it looks like.
        //
        // `taffy::TaffyTree::remove` nulls the parent pointer of every node in
        // the removed node's child list. A dying container's list still names
        // the span roots this frame reused, so removing it clears a pointer a
        // live container has just claimed. That is invisible until the reused
        // node is itself freed, at which point taffy's `children.retain`
        // cleans the wrong list and leaves a dead key inside a live container
        // — and removing THAT container panics with "invalid SlotMap key
        // used". Freeing parents before children avoids it: a dying node's
        // list-owner is its previous-frame parent, and a parent is always
        // dying when its child is, so the owner's list is dropped wholesale
        // before anything looks at it. Children are pushed before the node is
        // freed, because freeing clears its links.
        //
        // `tests/copy_forward_nested_churn.rs` drives the alternation that
        // would expose a regression here.
        let mut stack = vec![old_root];
        while let Some(n) = stack.pop() {
            if n.is_none() || n == tree.root() || !tree.is_alive(n) {
                continue;
            }
            let mut c = tree.first_child(n);
            while c.is_some() {
                stack.push(c);
                c = tree.next_sibling(c);
            }
            if let Some(raw) = tree.lnode(n) {
                layout.remove(LayoutNode::from_raw(raw));
            }
            meta.remove(&n);
            self.node_style.remove(&n);
            self.node_computed.remove(&n);
            tree.free_one(n);
        }
        debug_assert_eq!(
            layout.node_count(),
            tree.len(),
            "F2.2 layout leak: {} taffy nodes for {} arena nodes — a node was \
             neither reused nor freed",
            layout.node_count(),
            tree.len(),
        );
        debug_assert_eq!(
            meta.len(),
            tree.len(),
            "F2.2 meta leak: {} meta entries for {} arena nodes",
            meta.len(),
            tree.len(),
        );

        // F3.6: carry forward the bindings of nodes that were spliced rather
        // than re-lowered. A re-lowered node was allocated a fresh index and
        // its old one freed above, so "still alive" is exactly "spliced" —
        // the same test the span carry-forward below uses, for the same
        // reason. A build that re-lowered the node has already pushed a fresh
        // record for it.
        for b in prev_bg_bindings {
            if tree.is_alive(b.node) {
                self.bg_bindings.push(b);
            }
        }
        for b in prev_text_bindings {
            if tree.is_alive(b.node) {
                self.text_bindings.push(b);
            }
        }

        // F2.2: carry forward the span records of scopes that were never
        // visited this build because an ancestor took the memo-hit path.
        //
        // They still name the right nodes — nothing moved — so the test is
        // simply whether those nodes survived the free walk. A scope that
        // really did leave the view had its nodes re-lowered or dropped, so
        // its root is dead by now and the record is discarded. This replaces
        // the old per-span remap, which had to walk each copied span to find
        // the nested records and rewrite their roots.
        for (k, r) in &self.prev_spans {
            if !self.scope_spans.contains_key(k) && tree.is_alive(r.root) {
                self.scope_spans.insert(*k, *r);
            }
        }
        layout.compute(root_lnode, self.size);
        if self.rtl {
            layout.mirror_rtl(root_lnode);
        }

        // F2.2: bounds and clip for every live node this build could have
        // moved. A spliced span is not walked by the build, so its nodes are
        // not enumerated anywhere else — and their absolute positions still
        // change whenever something above them resizes.
        //
        // MUT3/MUT4: top-down with subtree pruning, replacing the flat O(live
        // nodes) pass. The layout engine's own pruner (`update_abs`) proves,
        // per subtree, that nothing moved — it compares the *unrounded*
        // absolute rect, so even a subpixel shift that keeps the rounded rect
        // descends — and `node_is_fresh` reports the result: `false` means
        // every stored bound and clip below the node is still exact, so the
        // walk stops there. A freshly lowered node always reports fresh (its
        // taffy slot is new), so its bounds and clip are always written; the
        // arena's build epoch (MUT3) cross-checks that as a debug invariant.
        // Scroll offsets need no special case: they are expressed through
        // layout (negative margins), so a scrolled span re-lowers and
        // descends. One narrowing this shares with restyle: a `.lss`
        // state-part that changes `clip` reaches the hit-test tree when the
        // node re-lowers, not from a spliced frame — restyle never wrote
        // `tree.clip` either, so that edge is unchanged.
        //
        // Clip propagation (unchanged): a `clip: true` node (e.g. a
        // Scrollable viewport) must reject pointer events on descendants that
        // overflow its box; descendants inherit the intersected clip in
        // `Tree::hit_test`, so only self-clipping nodes need it set. `.lss`
        // `clip` overrides the element flag (`none` disables it), mirroring
        // the paint clip in `emit_pass`.
        {
            let mut stack: Vec<NodeIndex> = vec![tree.root()];
            while let Some(node) = stack.pop() {
                if !node.is_some() {
                    continue;
                }
                stack.push(tree.next_sibling(node));
                if let Some(raw) = tree.lnode(node) {
                    let ln = LayoutNode::from_raw(raw);
                    if !layout.node_is_fresh(ln) {
                        debug_assert!(
                            !tree.born_this_epoch(node),
                            "a node lowered this build must have fresh layout"
                        );
                        continue; // prune: the whole subtree is current
                    }
                    let b = layout.bounds(ln);
                    tree.set_bounds(node, b);
                    let clip_on = self
                        .node_style
                        .get(&node)
                        .and_then(|s| s.clip)
                        .map(|c| c != lumen_style::StyleClip::None)
                        .unwrap_or_else(|| meta.get(&node).is_some_and(|m| m.clip));
                    tree.set_clip(node, clip_on.then_some(b));
                }
                stack.push(tree.first_child(node));
            }
        }
        // B.2b: container queries resolved against the *previous* layout's
        // container sizes; if this layout measured them differently, one
        // bounded re-pass lets queries see the fresh sizes within this pump
        // (a change caused *by* the re-pass itself waits for the next one —
        // prevents oscillation).
        let sizes: Vec<(f64, f64)> = self
            .container_nodes
            .iter()
            .map(|n| {
                let b = tree.bounds(*n);
                (b.width(), b.height())
            })
            .collect();
        if sizes != self.container_prev {
            self.container_prev = sizes;
            if !self.container_repass {
                self.container_repass = true;
                self.rebuild_inner();
                self.container_repass = false;
                // The re-pass stored its own scratch; this frame's is dead.
                return;
            }
        }

        self.layout_scratch = layout;
        self.tree = tree;
        self.meta = meta;
        // W1: a disabled node disables its whole subtree. Done as a pass over
        // the finished tree rather than threaded through `build_node`, so a
        // disabled container also covers children lowered by memo copy-forward.
        let root = self.tree.root();
        if root.is_some() {
            self.propagate_disabled(root, false);
        }
        // B.5: drop animations whose node id left the tree.
        //
        // This sweep used to build a `HashSet` of *every node's* id, cloned,
        // and it did it TWICE — once per registry — on every frame in which any
        // animation was live. That is O(nodes) string clones and hash inserts
        // to answer a question about one or two ids, and it was the dominant
        // cost of an animated frame: measured at ~6.5 us per node on a 6600-node
        // view whose memo was hitting perfectly (rebuilt=2, copied=6600), which
        // is a dropped frame at 60 Hz caused by a single spinner.
        //
        // The registries are tiny, so the membership test is inverted: clone
        // the few animating ids, then walk the nodes once and keep the ones
        // still present. Same answer, one pass, no per-node clone.
        if !self.prop_anims.is_empty() || !self.key_anims.is_empty() {
            let animating: std::collections::HashSet<StableId> = self
                .prop_anims
                .keys()
                .map(|(id, _)| id.clone())
                .chain(self.key_anims.keys().cloned())
                .collect();
            let mut live: std::collections::HashSet<StableId> =
                std::collections::HashSet::with_capacity(animating.len());
            for m in self.meta.values() {
                if let Some(id) = &m.id {
                    if live.len() < animating.len() && animating.contains(id) {
                        live.insert(id.clone());
                    }
                }
            }
            self.prop_anims.retain(|(id, _), _| live.contains(id));
            self.key_anims.retain(|id, _| live.contains(id));
        }
        self.last_damage = self.paint();
        *self.sem_root.borrow_mut() = None;
        self.invalidate_semantics_cache();
    }

    /// F4.2: the nodes depending on `signal`, gathered on demand.
    ///
    /// OB3: this used to be an eagerly-rebuilt reverse index
    /// (`HashMap<String, Vec<DepEntry>>`) refreshed at the end of *every*
    /// rebuild, cloning a `String` per dependency per node — for a structure
    /// whose only reader is `what_depends_on`, an agent RPC that is called
    /// interactively, if at all. A lean build paid for it too, despite having
    /// no agent surface at all.
    ///
    /// Scanning for one signal on demand is strictly better on every axis: no
    /// per-frame cost, nothing retained between frames, no `HashMap`
    /// allocation, and string clones only for the entries that actually match.
    /// The cost is O(nodes) per query instead of O(nodes) per frame.
    ///
    /// CP3.1: results are sorted by `(node, via)`. The scan visits `self.meta`,
    /// a `HashMap`, so iteration order is unspecified — without the sort the
    /// serialized `dependents` array would reorder whenever the hasher or the
    /// insertion pattern changed, silently altering agent-visible output.
    #[cfg(all(feature = "snapshot", feature = "dev-observability"))]
    fn dependents_of(&self, signal: &str) -> Vec<DepEntry> {
        let mut out: Vec<DepEntry> = Vec::new();
        for (node, m) in &self.meta {
            let node = node.index();
            let mut add = |keys: &[String], via, update| {
                if keys.iter().any(|k| k == signal) {
                    out.push(DepEntry { node, via, update });
                }
            };
            add(&m.deps.scope, "scope", "rebuild");
            add(&m.deps.text, "text", "rebuild");
            add(&m.deps.background, "background", "patch");
            add(&m.deps.class, "class", "rebuild");
        }
        out.sort_by(|a, b| a.node.cmp(&b.node).then_with(|| a.via.cmp(b.via)));
        out
    }

    /// Hash of everything *outside* a scope that its nodes' retained work
    /// depends on (A.3.2): the ancestor selector chain (descendant/child
    /// combinators + inherited hidden state), the enclosing container size
    /// (container queries), and overlay membership. Copy-forward is sound
    /// only when this matches the value recorded when the span was lowered.
    /// AN1: does any node in this span have a running transition/keyframe?
    ///
    /// Animations are keyed by `StableId`, so this is a lookup per node that
    /// has an id — and only ids can animate, so nodes without one are free.
    /// Cheap enough to run per copied span, and it replaces a global veto.
    fn span_has_running_anim(
        &self,
        nodes: &[NodeIndex],
        meta: &HashMap<NodeIndex, NodeMeta>,
    ) -> bool {
        if self.prop_anims.is_empty() && self.key_anims.is_empty() {
            return false; // the overwhelmingly common case
        }
        nodes.iter().any(|n| {
            let Some(id) = meta.get(n).and_then(|m| m.id.as_ref()) else {
                return false;
            };
            self.key_anims.get(id).is_some_and(|(_, done)| !done)
                || self
                    .prop_anims
                    .iter()
                    .any(|((aid, _), a)| aid == id && !a.committed)
        })
    }

    fn push_desc(&mut self, desc: std::rc::Rc<lumen_style::NodeDesc>) {
        self.desc_stack.push(desc);
        // Not computed here — see `desc_hash_stack`. A node that turns out to
        // be a leaf never has its descriptor hashed at all.
        self.desc_hash_stack.borrow_mut().push(None);
    }

    fn pop_desc(&mut self) {
        self.desc_stack.pop();
        self.desc_hash_stack.borrow_mut().pop();
        debug_assert_eq!(
            self.desc_hash_stack.borrow().len(),
            self.desc_stack.len() + 1,
            "the prefix stack must stay paired with the desc stack"
        );
    }

    /// The ancestor-chain prefix hash for the current `desc_stack` (O0.8),
    /// filling in any depths not yet computed.
    fn ancestor_prefix(&self) -> lumen_core::identity::IdHasher {
        use std::hash::Hash;
        let mut stack = self.desc_hash_stack.borrow_mut();
        debug_assert_eq!(stack.len(), self.desc_stack.len() + 1);
        if let Some(h) = stack[self.desc_stack.len()] {
            return h;
        }
        // Walk down to the deepest computed prefix, then hash forward,
        // memoizing each depth on the way back up. Amortized O(1) per node:
        // each descriptor is absorbed at most once per time it is pushed.
        let mut base = self.desc_stack.len();
        while stack[base].is_none() {
            base -= 1;
        }
        let mut h = stack[base].expect("loop exits on Some");
        for i in base..self.desc_stack.len() {
            let d = &self.desc_stack[i];
            d.id.hash(&mut h);
            d.classes.hash(&mut h);
            d.states.hash(&mut h);
            d.ty.hash(&mut h);
            stack[i + 1] = Some(h);
        }
        h
    }

    fn span_ctx_hash(&self, in_overlay: bool) -> IdHash {
        use std::hash::Hash;
        // F2.4: `IdHasher`, not `DefaultHasher`.
        //
        // This runs once per scope per build — 3000 times a frame on the
        // one-scope-per-row shape the F-series tells authors to write — and it
        // hashes the whole ancestor descriptor stack, so it is all string
        // traffic. `DefaultHasher` is SipHash-1-3, whose DoS resistance is
        // worthless for keys the build itself mints: it measured **8.3%** of a
        // memoized 3000-row frame (`sip::Hasher::write` 4.71% + `hash_one`
        // 3.61%), which is more than the whole splice path it guards.
        //
        // `IdHasher` is the project's own construction (ADR-021) — two
        // multiply-rotate lanes per word — and `finish128` gives 128 bits
        // rather than 64. That matters here and is not just tidiness: an equal
        // hash makes the runtime splice a span instead of re-lowering it, so a
        // collision is a wrong view, not a slow frame. Same trade R2 made for
        // `ShapeKey`.
        //
        // The value is compared only in memory, never serialized, so it is not
        // bound by ADR-021's stability rule either way.
        // O0.8: this walked the whole ancestor descriptor stack once per
        // node, hashing every ancestor's id, classes, states and role — so the
        // per-node style key was O(depth) of pure string traffic, 71 us/frame
        // on a flat 2000-row page and worse as views nest.
        //
        // Every input is stack-scoped: it changes when the build descends or
        // ascends, never between siblings. A list of 2000 rows therefore asked
        // this question 2000 times and got one answer. Now it computes once
        // per distinct context and the siblings read it back.
        let mut h = self.ancestor_prefix();
        if let Some(c) = self.container_stack.last().copied().flatten() {
            c.0.to_bits().hash(&mut h);
            c.1.to_bits().hash(&mut h);
        }
        in_overlay.hash(&mut h);
        (self.hidden_count > 0).hash(&mut h);
        (self.disabled_count > 0).hash(&mut h);
        let fast = h.finish128();
        // O0.8: an incremental hash that drifts from the one it replaced is
        // not a slow frame, it is a wrong view — an equal hash makes the
        // runtime splice a span instead of re-lowering it. So debug builds
        // recompute it the old way and compare, which puts the invariant
        // under every test in the suite rather than under the few that
        // happen to nest deeply.
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            fast,
            self.span_ctx_hash_from_scratch(in_overlay),
            "incremental ancestor-prefix hash diverged from the full walk"
        );
        fast
    }

    /// The pre-O0.8 computation, kept as the debug oracle for `span_ctx_hash`.
    #[cfg(debug_assertions)]
    fn span_ctx_hash_from_scratch(&self, in_overlay: bool) -> IdHash {
        use std::hash::Hash;
        let mut h = lumen_core::identity::IdHasher::new();
        for d in &self.desc_stack {
            d.id.hash(&mut h);
            d.classes.hash(&mut h);
            d.states.hash(&mut h);
            d.ty.hash(&mut h);
        }
        if let Some(c) = self.container_stack.last().copied().flatten() {
            c.0.to_bits().hash(&mut h);
            c.1.to_bits().hash(&mut h);
        }
        in_overlay.hash(&mut h);
        (self.hidden_count > 0).hash(&mut h);
        (self.disabled_count > 0).hash(&mut h);
        h.finish128()
    }

    /// Splice a memo-hit scope's span into this build (F2.2).
    ///
    /// The arena is retained across frames, so the span's nodes are already
    /// exactly right — same `NodeIndex`, same meta, same styles, same taffy
    /// nodes, same interaction flags. All that is left is to move the span
    /// *root* under its new parent; the subtree beneath it is not visited at
    /// all (F2.3), which is what makes a memo hit O(1) instead of O(span).
    ///
    /// Everything the old copy path did per node is now a non-event:
    ///
    /// * **Re-keying the side tables** — entries never move, because the node
    ///   keeps its index.
    /// * **Refreshing interaction flags** — the node never left the tree, and
    ///   `restyle_visual` keeps flags current on the live tree as pointer and
    ///   focus state change.
    /// * **Remapping nested span records** — nested spans still name the same
    ///   roots. They are carried forward wholesale at the end of the build.
    ///
    /// Returns `None` (caller lowers normally) if the span's nodes are gone,
    /// or if AN1 applies.
    fn splice_span(
        &mut self,
        key: IdHash,
        span: SpanRec,
        hash: IdHash,
        tree: &mut Tree,
        meta: &HashMap<NodeIndex, NodeMeta>,
        parent: Option<NodeIndex>,
    ) -> Option<(NodeIndex, LayoutNode)> {
        let root = span.root;
        if !tree.is_alive(root) {
            return None;
        }
        let lnode = LayoutNode::from_raw(tree.lnode(root)?);
        // AN1: refuse to splice a span containing an animating node — its
        // styles are mid-interpolation, so the retained work is stale.
        //
        // This is the one check that still needs the span's node list, so it
        // is gated on an animation actually running. With none (the
        // overwhelmingly common case) a memo hit touches one node.
        if !self.prop_anims.is_empty() || !self.key_anims.is_empty() {
            let nodes = tree.subtree_preorder(root);
            if self.span_has_running_anim(&nodes, meta) {
                return None;
            }
        }
        // Move the span under its new parent. `detach` is O(1) (F2.2 made the
        // child list doubly linked), and the previous parent — which is being
        // rebuilt — is left with a correct child list, so the free walk below
        // enumerates only dead nodes.
        tree.detach(root);
        match parent {
            Some(p) => tree.attach_last_child(p, root),
            None => tree.set_root(root),
        }
        self.scope_spans.insert(
            key,
            SpanRec {
                root,
                count: span.count,
                ctx_hash: hash,
                impure: false,
            },
        );
        self.nodes_copied += span.count;
        self.nodes_copied_total += span.count as u64;
        Some((root, lnode))
    }

    /// The node span a [`BuildCx::scope`](crate::BuildCx::scope) produced this
    /// build: its subtree-root node and preorder node count (A.3.1).
    /// Introspection for the retained-pipeline work and tests.
    ///
    /// `path` names the scope the way the build folded it (ADR-021) — a scope
    /// nested inside another is addressed by descending, not by spelling out a
    /// joined string key:
    ///
    /// ```ignore
    /// h.scope_span(ScopePath::root().child("list"));            // top-level
    /// h.scope_span(ScopePath::root().child("list").child("row-3")); // nested
    /// ```
    pub fn scope_span(&self, path: ScopePath) -> Option<(NodeIndex, u32)> {
        self.scope_spans
            .get(&path.hash())
            .map(|r| (r.root, r.count))
    }

    /// Set/replace the app stylesheet at runtime (tier-1 hot reload). A broken
    /// edit is rejected and the previous stylesheet stays live (04 §9).
    pub fn set_stylesheet(&mut self, src: &str) -> ReloadResult {
        let (sheet, diags) = lumen_style::parse("app.lss", src);
        if lumen_style::has_errors(&diags) {
            // C.2: reload rejections reach `app.logs` (the previous sheet
            // stays live, so the only other trace is stderr).
            self.rt.log(
                "warn",
                format!("stylesheet rejected ({} diagnostics)", diags.len()),
            );
            ReloadResult::Failed(diags)
        } else {
            self.app_sheet = Some(sheet);
            // A.5b: resolution results embed the sheet — invalidate the memo
            // (scope caches stay: cached Elements are pre-styling).
            self.style_memo.clear();
            self.rebuild();
            ReloadResult::Ok
        }
    }

    /// Switch the active theme and re-resolve styles.
    pub fn set_theme(&mut self, theme: lumen_style::ThemeKind) {
        // B.5b: seed the animation engine with every id-bearing node's
        // current colors so the 150 ms theme animation blends from the old
        // theme instead of snapping (nodes without css colors snap).
        if !self.reduced_motion {
            let seeds: Vec<(StableId, &'static str, AnimVal)> = self
                .meta
                .iter()
                .filter_map(|(node, m)| m.id.clone().map(|id| (node, id)))
                .flat_map(|(node, id)| {
                    let st = self.node_style.get(node);
                    let mut v = Vec::new();
                    if let Some(c) = st.and_then(|s| s.background) {
                        v.push((id.clone(), "background", AnimVal::Color(c)));
                    }
                    if let Some(c) = st.and_then(|s| s.color) {
                        v.push((id.clone(), "color", AnimVal::Color(c)));
                    }
                    v
                })
                .collect();
            for (id, prop, val) in seeds {
                self.prop_anims.entry((id, prop)).or_insert(PropAnim {
                    from: val,
                    to: val,
                    start_ms: self.clock_ms,
                    duration_ms: 0.0,
                    delay_ms: 0.0,
                    easing: lumen_style::Easing::Ease,
                    committed: true,
                });
            }
            self.theme_anim_until = self.clock_ms + 150.0;
        }
        self.theme = theme;
        // A.5b: token tables are theme-scoped — resolution memo out.
        self.style_memo.clear();
        self.rebuild();
    }

    /// Set the theme by name (`"light"|"dark"|"high-contrast"`).
    pub fn set_theme_str(&mut self, theme: &str) {
        let t = match theme {
            "dark" => lumen_style::ThemeKind::Dark,
            "high-contrast" => lumen_style::ThemeKind::HighContrast,
            _ => lumen_style::ThemeKind::Light,
        };
        self.set_theme(t);
    }

    /// Computed styles for the node a `selector` resolves to (03 §3 ui.getStyles,
    /// 04 §7 value serialization). Returns `null` if the selector doesn't resolve
    /// to exactly one node. Snapshot builds only (the agent introspection path).
    #[cfg(feature = "snapshot")]
    pub fn get_styles(&self, selector: &str) -> serde_json::Value {
        let root = self.semantics_elided();
        let Ok(id) = lumen_core::semantics::resolve_one(&root, selector) else {
            return serde_json::Value::Null;
        };
        let node = self.node_for_handle(id);
        let Some(node) = node else {
            return serde_json::Value::Null;
        };
        let mut map = serde_json::Map::new();
        if let Some(computed) = self.node_computed.get(&node) {
            for (prop, c) in computed.iter() {
                map.insert(
                    prop.clone(),
                    lumen_style::computed_json_spanned(&c.value, c.origin, c.span),
                );
            }
        }
        serde_json::Value::Object(map)
    }

    /// The reactive dependencies of the node a `selector` resolves to (F4
    /// `ui.getDeps`): the union of signal keys plus a per-prop breakdown
    /// (`scope`, `text`, `background`). `null` if the selector doesn't resolve to
    /// exactly one node. Snapshot builds only.
    ///
    /// A11Y3: also requires `dev-observability`, which is what collects the
    /// per-node dep keys this reports. A snapshot build without it keeps every
    /// other `ui.*` query and loses only this one.
    #[cfg(all(feature = "snapshot", feature = "dev-observability"))]
    pub fn get_deps(&self, selector: &str) -> serde_json::Value {
        let root = self.semantics_elided();
        let Ok(id) = lumen_core::semantics::resolve_one(&root, selector) else {
            return serde_json::Value::Null;
        };
        let node = self.node_for_handle(id);
        let Some(deps) = node.and_then(|n| self.meta.get(&n)).map(|m| &m.deps) else {
            return serde_json::Value::Null;
        };
        serde_json::json!({
            "node": id.to_wire(),
            "deps": deps.union(),
            "byProp": {
                "scope": deps.scope,
                "text": deps.text,
                "background": deps.background,
                "class": deps.class,
            },
        })
    }

    /// The nodes that depend on `signal` and how they'd update if it changed
    /// (F4.2 `ui.whatDependsOn`) — predictive, no write. Empty for a signal the
    /// view doesn't read. Snapshot builds only.
    ///
    /// A11Y3: like `get_deps`, also requires `dev-observability` — the reverse
    /// index it scans is the per-node dep keys that feature collects.
    #[cfg(all(feature = "snapshot", feature = "dev-observability"))]
    pub fn what_depends_on(&self, signal: &str) -> serde_json::Value {
        let dependents: Vec<serde_json::Value> = self
            .dependents_of(signal)
            .iter()
            .map(|e| {
                serde_json::json!({
                    // e.node is an arena index (dependents_of scans meta);
                    // translate so the agent gets a usable selector.
                    "node": self.handle_for_index(e.node).map(|h| h.to_wire()),
                    "via": e.via,
                    "update": e.update,
                })
            })
            .collect();
        serde_json::json!({ "signal": signal, "dependents": dependents })
    }

    /// What the last `pump` did (F4.3 `ui.lastChange`): `kind` is
    /// `idle`/`patch`/`rebuild`; `nodes` are the exact patched nodes (a rebuild
    /// reports none — read the fresh tree via `getTree`). Snapshot builds only.
    #[cfg(feature = "snapshot")]
    pub fn last_change(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.last_change.kind,
            "nodes": self
                .last_change
                .nodes
                .iter()
                .filter_map(|n| self.handle_for_index(*n).map(|h| h.to_wire()))
                .collect::<Vec<_>>(),
        })
    }

    /// Activate a control by running its retained handler directly (F4.4),
    /// instead of synthesizing a pointer at its centre and re-hit-testing — more
    /// robust under overlap/transforms. `action` is `click`/`focus`/`dismiss`.
    /// Pumps afterward; returns the node index or an error string.
    pub fn invoke_action(
        &mut self,
        selector: &str,
        action: &str,
    ) -> Result<lumen_core::identity::NodeHandle, String> {
        self.invoke_action_with(selector, action, None)
    }

    /// [`Headless::invoke_action`] with a payload — `setValue` carries the new
    /// value as a string, which the widget parses (W2).
    pub fn invoke_action_with(
        &mut self,
        selector: &str,
        action: &str,
        value: Option<&str>,
    ) -> Result<lumen_core::identity::NodeHandle, String> {
        let root = self.semantics_elided();
        let id = lumen_core::semantics::resolve_one(&root, selector)
            .map_err(|_| format!("selector `{selector}` did not resolve to one node"))?;
        let node = self
            .node_for_handle(id)
            .ok_or_else(|| "resolved node is not live".to_string())?;
        // W1: the agent gets the same answer as the pointer. Without this the
        // geometry-free path would drive a control the user cannot touch.
        if self.is_disabled(node) {
            return Err(format!("node `{selector}` is disabled"));
        }
        let m = self.meta.get(&node);
        match action {
            "click" => {
                let handler = m.and_then(|m| m.on_click.clone());
                match handler {
                    Some(h) => h(&self.rt),
                    None => return Err(format!("node `{selector}` has no click handler")),
                }
            }
            "focus" => self.focused_id = m.and_then(|m| m.id.clone()),
            "dismiss" => {
                let handler = m.and_then(|m| m.on_dismiss().cloned());
                if let Some(h) = handler {
                    h(&self.rt);
                }
            }
            // W2: value adjustment. A widget that declares Increment/Decrement/
            // SetValue must implement it — otherwise the semantic tree
            // advertises a capability neither the agent nor a screen reader can
            // use, which is exactly the drift ADR-009 exists to prevent.
            "increment" => {
                let handler = m.and_then(|m| m.on_increment().cloned());
                match handler {
                    Some(h) => h(&self.rt),
                    None => return Err(format!("node `{selector}` has no increment handler")),
                }
            }
            "decrement" => {
                let handler = m.and_then(|m| m.on_decrement().cloned());
                match handler {
                    Some(h) => h(&self.rt),
                    None => return Err(format!("node `{selector}` has no decrement handler")),
                }
            }
            "setValue" | "set_value" => {
                let handler = m.and_then(|m| m.on_set_value().cloned());
                let v = value.ok_or_else(|| "`setValue` needs a `value`".to_string())?;
                match handler {
                    Some(h) => h(&self.rt, v),
                    None => return Err(format!("node `{selector}` has no setValue handler")),
                }
            }
            other => return Err(format!("unsupported action `{other}`")),
        }
        self.pump();
        Ok(id)
    }
}

/// What a widget writes through — [`Sink`] with its type parameters erased.
///
/// `Sink` is generic over renderer, executor and platform, and a widget has no
/// business knowing any of them. Erasing them here keeps `Direct` free of type
/// parameters, which is also what makes an object-safe companion possible for
/// the escape-hatch tier. The dispatch is per *node*, not per field, so it is
/// one indirect call against everything a node costs to write.
pub trait NodeWriter {
    /// Write a childless node.
    fn write_leaf(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode);

    /// Lower a complete `Element` tree, children included.
    ///
    /// The bridge every widget starts on: a widget that has not yet been
    /// converted to native lowering still builds its `Element` and hands the
    /// whole thing over here. It is what makes "every widget is `Direct`" true
    /// before "no widget builds an `Element`" is.
    fn write_tree(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode);

    /// Write a node and lower `children` under it, applying the node's own
    /// contexts.
    ///
    /// The shape almost every container wants: it owns a list of child
    /// `Element`s and imposes nothing on them beyond what the node itself
    /// declares. Provided rather than hand-written per widget so the context
    /// handling — a z-stack's absolute positioning, today — lives in one place
    /// and cannot be forgotten by the next container to convert.
    fn write_children(
        &mut self,
        el: Element,
        children: Vec<Element>,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        let stacks = el.stacks_children;
        let mut kids = Some(children);
        self.write_with(el, parent, in_overlay, &mut |w, node, overlay| {
            kids.take()
                .expect("a node's children are lowered exactly once")
                .into_iter()
                .map(|mut c| {
                    if stacks {
                        c.style.position = lumen_layout::Position::Absolute;
                        c.style.inset = lumen_layout::Edges {
                            left: Dim::px(0.0),
                            top: Dim::px(0.0),
                            ..lumen_layout::Edges::AUTO
                        };
                    }
                    w.write_tree(c, Some(node), overlay).1
                })
                .collect()
        })
    }

    /// Write a node whose children are emitted by a **statement-form body**.
    ///
    /// The body runs during lowering, not during view construction, which is
    /// what lets it write each child straight through and never collect them.
    fn write_body(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
        body: &mut dyn FnMut(&mut Kids),
    ) -> (NodeIndex, LayoutNode) {
        self.write_with(el, parent, in_overlay, &mut |w, node, overlay| {
            let mut lns = Vec::new();
            let mut kids = Kids {
                w,
                node,
                in_overlay: overlay,
                lns: &mut lns,
            };
            body(&mut kids);
            lns
        })
    }

    /// Write a node whose children are produced *while it is open*.
    ///
    /// `el.children` is ignored; the callback supplies them. This is what lets
    /// a container emit children as statements instead of collecting them into
    /// a vector first.
    fn write_with(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
        children: &mut dyn FnMut(&mut dyn NodeWriter, NodeIndex, bool) -> Vec<LayoutNode>,
    ) -> (NodeIndex, LayoutNode);
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig> NodeWriter
    for Sink<'_, R, E, P>
{
    fn write_leaf(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        self.lower_node(el, parent, in_overlay, |_, _, _, _| Vec::new())
    }

    fn write_tree(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        self.build_node(el, parent, in_overlay)
    }

    fn write_with(
        &mut self,
        el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
        children: &mut dyn FnMut(&mut dyn NodeWriter, NodeIndex, bool) -> Vec<LayoutNode>,
    ) -> (NodeIndex, LayoutNode) {
        self.lower_node(el, parent, in_overlay, |sink, node, overlay, _stacks| {
            children(sink, node, overlay)
        })
    }
}

impl Direct for Element {
    /// An `Element` is itself a `Direct` widget — it writes its whole tree.
    ///
    /// This is what makes the authoring change additive rather than breaking:
    /// every `fn build(cx) -> Element` view in existence is already a view
    /// returning something `Direct`, so a signature that accepts `impl Direct`
    /// accepts all of them unchanged, and statement-form views can be adopted
    /// one call site at a time.
    fn lower_owned(
        self,
        w: &mut dyn NodeWriter,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        w.write_tree(self, parent, in_overlay)
    }
}

/// The handle a statement-form container's body writes children into.
///
/// Each `child` is lowered **immediately**, while the parent node is open —
/// so the children of a container are never collected anywhere. That is the
/// difference between `column(vec![a, b, c])`, which materializes a vector of
/// nodes before any of them is written, and `column(|c| { c.child(a); … })`,
/// which writes each one and moves on.
pub struct Kids<'w, 'n> {
    w: &'w mut dyn NodeWriter,
    node: NodeIndex,
    in_overlay: bool,
    lns: &'n mut Vec<LayoutNode>,
}

impl Kids<'_, '_> {
    /// Write one child, now.
    ///
    /// Monomorphic: a widget whose type is known here never becomes a trait
    /// object, so this inlines and costs nothing beyond the write.
    pub fn child<W: Direct>(&mut self, w: W) {
        let (_, ln) = w.lower_owned(self.w, Some(self.node), self.in_overlay);
        self.lns.push(ln);
    }

    /// The parent this is writing into, for a widget that needs the primitives
    /// directly.
    pub fn writer(&mut self) -> (&mut dyn NodeWriter, NodeIndex, bool) {
        (self.w, self.node, self.in_overlay)
    }
}

/// A stored root view: a closure producing the frame's root widget, erased.
pub type RootView = Box<dyn Fn(&mut BuildCx) -> Box<dyn DirectDyn>>;

/// A widget that writes itself into the tree, with no `Element` subtree.
///
/// The by-value half: a widget whose type is known at the call site never
/// becomes a trait object, so `child` calls inline and cost nothing beyond the
/// write itself.
pub trait Direct: Sized {
    /// Write this widget and its subtree under `parent`.
    fn lower_owned(
        self,
        w: &mut dyn NodeWriter,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode);
}

/// The object-safe face of [`Direct`], for the one case that needs it: a
/// heterogeneous collection of children a container holds and edits before
/// they lower. Everything else should use `Direct` directly and pay nothing.
pub trait DirectDyn {
    /// Lower the widget this slot holds. Panics if called twice.
    fn lower_dyn(
        &mut self,
        w: &mut dyn NodeWriter,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode);
}

impl<W: Direct> DirectDyn for Option<W> {
    fn lower_dyn(
        &mut self,
        w: &mut dyn NodeWriter,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        self.take()
            .expect("a node lowers exactly once")
            .lower_owned(w, parent, in_overlay)
    }
}

/// The destination a node is written into, plus the engine state a write needs.
///
/// Before this existed, `build_node` *was* the only way to produce a node: the
/// writes and the `Element` reads were one 690-line function, so a widget could
/// not write a node without first constructing an `Element` to be read out of.
/// Splitting the destination from the source is what lets the two lowering
/// paths coexist — the `Element` path and [`Direct`] widgets write through the
/// same primitives, so widgets can migrate one at a time instead of all at
/// once.
///
/// The four fields are disjoint borrows: `app` is the engine, and the tree,
/// layout and meta are the per-rebuild destinations `rebuild_inner` holds
/// locally while a build is in flight.
pub struct Sink<'a, R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig> {
    pub(crate) app: &'a mut Headless<R, E, P>,
    pub(crate) tree: &'a mut Tree,
    pub(crate) layout: &'a mut P::Layout,
    pub(crate) meta: &'a mut HashMap<NodeIndex, NodeMeta>,
    /// T2: is the **containing block's** width definite for the node being
    /// lowered? A percentage resolves against it, so a percentage under an
    /// indefinite parent is itself indefinite. True at the root — the root's
    /// containing block is the viewport, which always has a width.
    pub(crate) cb_definite: bool,
    /// T2: does the parent assign this node's width (a flex column with the
    /// default stretch cross-alignment)? An `Auto` width under such a parent is
    /// decided by the parent rather than by the node's own content, which is
    /// what makes measuring that content pointless.
    pub(crate) stretched: bool,
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig>
    Sink<'_, R, E, P>
{
    /// Lower one `Element` and its subtree.
    ///
    /// The `Element` path, now expressed as a client of [`Sink`] rather than as
    /// the only way to write a node. A [`Direct`] widget writes through the
    /// same primitives, which is what lets the two coexist while widgets
    /// migrate one at a time.
    /// Lower the root view.
    ///
    /// The root arrives boxed — its concrete type was erased when the closure
    /// was stored — so this is the one node per frame that goes through the
    /// erased path. Everything below it is monomorphic.
    pub(crate) fn lower_root(&mut self, mut root: Box<dyn DirectDyn>) -> (NodeIndex, LayoutNode) {
        root.lower_dyn(self, None, false)
    }

    /// Lower one `Element` and its subtree.
    pub(crate) fn build_node(
        &mut self,
        mut el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
    ) -> (NodeIndex, LayoutNode) {
        // The migration boundary: this `Element` stands in for a `Direct`
        // widget, so hand the write to it rather than lowering the placeholder.
        if let Some(slot) = el.rare.as_mut().and_then(|r| r.direct.take()) {
            let taken = slot.borrow_mut().take();
            if let Some(mut w) = taken {
                return w.lower_dyn(self, parent, in_overlay);
            }
        }
        let kids = std::mem::take(&mut el.children);
        self.lower_node(el, parent, in_overlay, |sink, node, overlay, stacks| {
            kids.into_iter()
                .map(|mut c| {
                    if stacks {
                        c.style.position = lumen_layout::Position::Absolute;
                        c.style.inset = lumen_layout::Edges {
                            left: Dim::px(0.0),
                            top: Dim::px(0.0),
                            ..lumen_layout::Edges::AUTO
                        };
                    }
                    sink.build_node(c, Some(node), overlay).1
                })
                .collect()
        })
    }

    /// Write one node, with its children supplied by a **callback** rather than
    /// carried in a vector.
    ///
    /// This is the inversion the whole migration turns on. While children were
    /// a `Vec<Element>` field, a parent could not exist without its entire
    /// subtree existing first, so the peak cost of a frame was the whole tree
    /// of 784-byte records alive at once. As a callback, a node's children are
    /// lowered *while it is open* and never held: an `Element` becomes a
    /// transient per-node parameter block instead of a tree, and a widget that
    /// emits its children as statements never builds one at all.
    ///
    /// `el.children` is ignored — the caller has already taken it.
    pub(crate) fn lower_node<F>(
        &mut self,
        mut el: Element,
        parent: Option<NodeIndex>,
        in_overlay: bool,
        children: F,
    ) -> (NodeIndex, LayoutNode)
    where
        F: FnOnce(&mut Self, NodeIndex, bool, bool) -> Vec<LayoutNode>,
    {
        // A.3.1: a scope-root element records its node span. Nodes allocate
        // preorder in the fresh per-rebuild tree, so a subtree is the
        // contiguous range [span_start, self.tree.len()) once its children are
        // lowered — the anchor the retained-graph splice (A.3.3) replaces.
        // Taken before the children are consumed (partial-move below).
        let span_start = self.tree.len();
        // A.3.2: a memo-hit stub — either copy the scope's span forward from
        // the previous build (sound iff the recorded outside-context hash
        // matches and the span had no per-node side work), or materialize an
        // owned clone of the cached subtree and lower it normally.
        if let Some(rc) = el.shared.take() {
            let key = el.scope_key.expect("shared stub carries its key");
            let hash = self.app.span_ctx_hash(in_overlay);
            if self.app.allow_copy_forward {
                if let Some(span) = self.app.prev_spans.get(&key).copied() {
                    if !span.impure && span.ctx_hash == hash {
                        if let Some(res) = self
                            .app
                            .splice_span(key, span, hash, self.tree, self.meta, parent)
                        {
                            return res;
                        }
                    }
                }
            }
            let mut owned = (*rc).clone();
            owned.scope_key = Some(key);
            el = owned;
        }
        // The disabled wash, imposed as this node is written. `disabled_count`
        // is the ancestors' depth; `el.disabled` is this node's own. Applied
        // before the cascade, so a `.lss` `:disabled` rule still overrides it,
        // which is the ordering the recursive walk had.
        if el.disabled || self.app.disabled_count > 0 {
            crate::element::mute_node(&mut el);
        }
        let span_key = el.scope_key.take();
        let span_hash = span_key
            .map(|_| self.app.span_ctx_hash(in_overlay))
            .unwrap_or(lumen_core::identity::ROOT_ID);
        let impure_at = self.app.impure_seen;
        // F3.6: `dyn_text` and `dyn_bg` used to be listed here, which barred
        // every span containing one from the splice path. That was the right
        // rule while a binding could only be refreshed by re-lowering its node:
        // a spliced span reuses last frame's `meta`, so a binding whose signal
        // had moved would come back stale.
        //
        // `settle_bindings_for_rebuild` removes the premise — a stale binding
        // is now brought up to date before the build starts, or the caches are
        // dropped so nothing splices at all. Keeping the ban would mean a
        // single bound label anywhere in a list makes the whole list re-lower,
        // which is precisely the cost that made authors avoid bindings.
        //
        // `dyn_classes` stays: classes drive the `.lss` cascade, so a change
        // can resize anything in the subtree, and there is no cheap check for
        // "would this cascade differently". `Custom`/`Canvas` stay because they
        // are arbitrary closures whose output cannot be predicted at all.
        if el.dyn_classes.is_some()
            || matches!(
                el.content,
                NodeContent::Custom(..) | NodeContent::Canvas(..)
            )
        {
            self.app.impure_seen += 1;
        }
        self.app.nodes_rebuilt += 1;
        self.app.nodes_rebuilt_total += 1;
        // F2.2: the tree is retained, so the previous frame's root is still
        // present while this one is being built — `insert_root` would assert.
        // The new node is created detached and claims the root afterwards; the
        // old root is freed by the walk at the end of the rebuild.
        let node = match parent {
            None => {
                let n = self.tree.insert_orphan();
                self.tree.set_root(n);
                n
            }
            Some(p) => self.tree.insert_child(p),
        };
        // Overlay subtrees (dropdown menus, popovers, tooltips) paint in a final
        // top pass that escapes ancestor clips. Hit-testing keys on `z` first, so
        // give them an elevated z to match — otherwise content that paints *under*
        // the overlay but comes later in document order would steal its clicks.
        let this_overlay = in_overlay || el.overlay;

        // F3: evaluate reactive prop bindings *before* the content is read for
        // hit-testing/measurement, recording their dependency keys per prop (F4).
        // A11Y3: the dep *key* vectors exist only for `ui.getDeps`. Reactivity
        // itself runs off the `ReadSet`s below, which are kept in both states —
        // `dep_keys` is a `Vec<String>` built per bound node purely so an agent
        // can name the signals, so a build with no agent skips it.
        #[cfg(feature = "dev-observability")]
        let mut text_deps: Vec<String> = Vec::new();
        // F3.5: the binding, held until the sizing block below has measured it
        // — that is where the wrap width and the auto-size flags are known.
        let mut pending_text: Option<(lumen_core::Dynamic<String>, lumen_core::state::ReadSet)> =
            None;
        #[cfg(feature = "dev-observability")]
        let mut bg_deps: Vec<String> = Vec::new();
        #[cfg(feature = "dev-observability")]
        let mut class_deps: Vec<String> = Vec::new();
        if el.dyn_text.is_some() || el.dyn_bg.is_some() || el.dyn_classes.is_some() {
            let rt = self.app.rt.clone();
            if let Some(d) = el.dyn_classes.clone() {
                // Classes drive the `.lss` cascade (may change size) → NON-isolated
                // (structural). Appended to the static classes.
                let (classes, reads) = d.eval(&rt);
                #[cfg(feature = "dev-observability")]
                {
                    class_deps = reads.dep_keys(&rt);
                }
                self.app.structural_reads.extend(&reads);
                el.classes.extend(classes);
            }
            if let Some(d) = el.dyn_text.clone() {
                // F3.5: ISOLATED, like the background binding. Text used to be
                // structural on the grounds that a new string can measure to a
                // new size — true of some values, not of the binding. The
                // measurement below decides per update, and a change that does
                // move layout falls back to a rebuild there.
                //
                // Isolating the reads cannot strand a memoized subtree with
                // stale text. F3.5 relied on the `impure` rule above for that;
                // F3.6 removed it, and the guarantee now comes from
                // `settle_bindings_for_rebuild`, which refreshes every stale
                // binding before a rebuild chooses what to splice — and drops
                // the view caches outright when a refresh would move self.layout.
                let (s, reads) = d.eval_isolated(&rt);
                #[cfg(feature = "dev-observability")]
                {
                    text_deps = reads.dep_keys(&rt);
                }
                pending_text = Some((d, reads));
                // The string is the node's content *and* its accessible label
                // (Element::text sets both); keep them in sync.
                el.label = s.clone();
                el.content = match std::mem::take(&mut el.content) {
                    NodeContent::Text(_, ts) => NodeContent::Text(s, ts),
                    _ => NodeContent::Text(s, lumen_text::TextStyle::default()),
                };
            }
            if let Some(d) = el.dyn_bg.clone() {
                // Background is paint-only → ISOLATED + retained: a change patches
                // this node in place without a rebuild (F3.4).
                let (c, reads) = d.eval_isolated(&rt);
                #[cfg(feature = "dev-observability")]
                {
                    bg_deps = reads.dep_keys(&rt);
                }
                el.background = Some(c);
                self.app.bg_bindings.push(BoundBg {
                    node,
                    dynamic: d,
                    deps: reads,
                });
            }
        }
        #[cfg(feature = "dev-observability")]
        let node_deps = NodeDeps {
            scope: el.scope_deps.take().unwrap_or_default(),
            text: text_deps,
            background: bg_deps,
            class: class_deps,
        };

        let mut flags = NodeFlags::VISIBLE;
        let interactive = el.background.is_some()
            || el.on_click.is_some()
            || matches!(
                el.content,
                NodeContent::Text(..) | NodeContent::Image(..) | NodeContent::Custom(..)
            )
            || el.get_on_wheel().is_some()
            || el.get_on_drag().is_some()
            || el.get_on_key().is_some()
            || el.focusable;
        if interactive {
            flags |= NodeFlags::HIT_TESTABLE;
        }
        if el.focusable {
            flags |= NodeFlags::FOCUSABLE;
        }
        if el.disabled {
            flags |= NodeFlags::DISABLED;
        }
        if el.id.is_some() && el.id == self.app.focused_id {
            flags |= NodeFlags::FOCUSED;
        }
        if el.id.is_some() && el.id == self.app.hovered_id {
            flags |= NodeFlags::HOVERED;
        }
        self.tree.set_flags(node, flags);
        if this_overlay {
            self.tree.set_z(node, OVERLAY_Z);
        }

        // A.2: resolve this node's `.lss` rules *now*, before anything
        // consumes `el.style` (text wrap, custom measure, taffy), so layout
        // properties from the stylesheet are real. Per-node resolution needs
        // no ancestry (compound selectors), dynamic classes were merged into
        // `el.classes` above, and the visual states are known from the flags
        // just computed. Paint properties land in `node_style`/
        // `node_computed` exactly as the old post-layout pass produced them
        // (`emit_pass`/`get_styles` are unchanged consumers).
        //
        // NOTE for A.3.2 (retained scopes): this mutates the *owned* element;
        // once memo hits become shared `Rc` subtrees the merge must move to a
        // per-node copy instead.
        let mut pushed_hidden = false;
        if let Some(env) = &self.app.style_env {
            // B.6a: the full state vocabulary — interaction states carry
            // their CSS-familiar aliases (spec examples write `:hover`), and
            // the widget's semantic states (checked/disabled/expanded/…)
            // are style-matchable, so `checkbox:checked { … }` just works.
            // O0.11: `&str`, not `String`. Every entry here is either a
            // literal or already owned by the element, and the only consumer
            // that needs owned data is the `NodeDesc` built on a memo MISS —
            // which is the rare path. The `Vec` itself still allocates when
            // non-empty, but the overwhelmingly common node has no states at
            // all and now allocates nothing for them.
            let mut states: Vec<&str> = Vec::new();
            if flags.contains(NodeFlags::FOCUSED) {
                states.push("focused");
                states.push("focus");
            }
            if flags.contains(NodeFlags::HOVERED) {
                states.push("hovered");
                states.push("hover");
            }
            if el.id.is_some()
                && self
                    .app
                    .pressed
                    .as_ref()
                    .is_some_and(|(_, id)| *id == el.id)
            {
                states.push("pressed");
                states.push("active");
            }
            states.extend(el.states.iter().map(|s| s.as_str()));
            // W1: `disabled` is its own Element field (not a semantic state the
            // author writes), so fold it in here — inherited, so a control
            // inside a disabled container matches `:disabled` too.
            if el.disabled || self.app.disabled_count > 0 {
                states.push("disabled");
            }
            let node_ty = el.role.as_str();
            // B.1: the recursion's ancestor chain makes descendant/`>`
            // selectors real (previously only the rightmost compound was
            // checked — `dialog button` matched every button). B.2: the live
            // media context gates `@media` blocks on the actual window.
            // B.2b: inside a `.container()`, container queries test that
            // ancestor's size (from the last layout) instead of the window.
            let media = match self.app.container_stack.last().copied().flatten() {
                Some(size) => std::borrow::Cow::Owned(lumen_style::MediaContext {
                    container: Some(size),
                    ..env.media.clone()
                }),
                None => std::borrow::Cow::Borrowed(&env.media),
            };
            // A.5b: resolution is a pure function of (desc, ancestor chain,
            // container size) for a fixed sheet/theme/media — memoize it.
            // O0.11: hashed from the PARTS, so the descriptor itself need not
            // exist yet. Building it costs three allocations (the id string,
            // the class vector, the role string) for a value that is identical
            // across every node with the same style identity — which is
            // exactly the set this key already collapses. So the descriptor
            // joins the memo entry and a hit returns it as a refcount bump.
            let style_key = {
                use std::hash::Hash;
                // F2.4: same swap, same reasoning as `span_ctx_hash` — this
                // one runs per node per build whenever a stylesheet is loaded,
                // and a collision hands a node another node's resolved style.
                // The stream is length-prefixed per field so that
                // `["a","b"]` and `["ab"]` cannot collide.
                let mut h = lumen_core::identity::IdHasher::new();
                match el.id.as_ref() {
                    Some(i) => {
                        1u8.hash(&mut h);
                        i.as_str().hash(&mut h);
                    }
                    None => 0u8.hash(&mut h),
                }
                el.classes.len().hash(&mut h);
                for c in &el.classes {
                    c.as_str().hash(&mut h);
                }
                states.len().hash(&mut h);
                for st in &states {
                    st.hash(&mut h);
                }
                node_ty.hash(&mut h);
                self.app.span_ctx_hash(this_overlay).hash(&mut h);
                h.finish128()
            };
            let (desc, mut css, mut resolved) = if let Some(e) = self.app.style_memo.get(&style_key)
            {
                self.app.style_memo_hits += 1;
                e.clone()
            } else {
                self.app.style_memo_misses += 1;
                let desc = std::rc::Rc::new(lumen_style::NodeDesc {
                    id: el.id.as_ref().map(|i| i.as_str().to_string()),
                    classes: el.classes.clone(),
                    states: states.iter().map(|s| s.to_string()).collect(),
                    ty: node_ty.to_string(),
                });
                // The ancestor chain is held as `Rc`s; `resolve_with_ancestors`
                // wants a plain slice. Materializing it here rather than
                // changing that signature keeps the cost on the miss path,
                // which is O(depth) and rare, instead of on every node.
                let ancestors: Vec<lumen_style::NodeDesc> =
                    self.app.desc_stack.iter().map(|d| (**d).clone()).collect();
                let computed =
                    lumen_style::resolve_with_ancestors(&env.sources, &desc, &ancestors, &media);
                let mut css = lumen_style::Style::new();
                let mut resolved = HashMap::default();
                for (prop, c) in &computed {
                    lumen_style::apply(&mut css, prop, &c.value, &env.tokens);
                    // Store the token-resolved value so `get_styles` returns
                    // the computed (substituted) form (04 §7).
                    resolved.insert(
                        prop.clone(),
                        lumen_style::Computed {
                            value: lumen_style::resolve_token(&c.value, &env.tokens),
                            important: c.important,
                            origin: c.origin,
                            span: c.span,
                        },
                    );
                }
                let resolved: Computeds = std::rc::Rc::new(resolved);
                let css: Styled = std::rc::Rc::new(css);
                self.app.style_memo.insert(
                    style_key,
                    (
                        std::rc::Rc::clone(&desc),
                        std::rc::Rc::clone(&css),
                        std::rc::Rc::clone(&resolved),
                    ),
                );
                (desc, css, resolved)
            };
            // B.6b: the typed inline style is the `Origin::Inline` tier —
            // merged after the (memoized) sheet resolution, field-wise, and
            // yielding to `!important` sheet declarations (04 §2). The merge
            // runs before the layout override below, so inline layout
            // properties win there too.
            if let Some(inline) = el.css_inline.as_deref() {
                // O0.6/O0.10: an inline style is this node's alone — fork both
                // shared halves before writing into them.
                merge_inline_style(
                    std::rc::Rc::make_mut(&mut css),
                    std::rc::Rc::make_mut(&mut resolved),
                    inline,
                );
            }
            // B.5: substitute mid-flight transition blends (and start/retarget
            // segments) before anything consumes the style; then play any
            // `animation:` timeline on top.
            //
            // O0.10: both are no-ops unless the style *itself* says otherwise —
            // `apply_transitions` returns immediately without `transitions` (or
            // a theme window), `apply_keyframes` without an `animation`. Those
            // are exactly the conditions under which the shared style would
            // have to be forked, so testing them here keeps the overwhelmingly
            // common node on a refcount bump instead of a 1008-byte copy.
            // Re-read AFTER the inline merge, which can introduce either.
            let wants_transition =
                !css.transitions.is_empty() || self.app.clock_ms < self.app.theme_anim_until;
            let wants_keyframes = css.animation.is_some();
            if wants_transition || wants_keyframes {
                let owned = std::rc::Rc::make_mut(&mut css);
                if wants_transition {
                    self.app.apply_transitions(&el.id, owned);
                }
                if wants_keyframes {
                    self.app.apply_keyframes(&el.id, owned);
                }
            }
            apply_css_to_element(&mut el, &css);
            // B.3 visibility: a hidden node (or one inside a hidden
            // subtree) keeps its layout space but leaves hit-testing (flags)
            // and, via the paint partition, rendering + semantics.
            if css.visibility == Some(false) {
                self.app.hidden_count += 1;
                pushed_hidden = true;
            }
            if self.app.hidden_count > 0 {
                self.tree.set_flags(node, NodeFlags::empty());
            }
            // PROP1 `z-index`: applied once the cascade has resolved.
            // Overlay roots keep OVERLAY_Z — they route to the overlay pass
            // regardless, and a stylesheet must not be able to demote a
            // dropdown under the page.
            if !this_overlay {
                if let Some(z) = css.z_index {
                    self.tree.set_z(node, z.max(0) as u32);
                }
            }
            self.app.node_style.insert(node, css);
            self.app.node_computed.insert(node, resolved);
            // B.1: this node becomes an ancestor for its children's matching
            // (popped after the recursion below).
            self.app.push_desc(desc);
        } else if let Some(inline) = el.css_inline.as_deref().cloned() {
            // B.6b without a stylesheet: the inline tier still applies (its
            // own layout/typography/visibility effects included).
            let mut css = lumen_style::Style::new();
            let mut resolved = HashMap::default();
            merge_inline_style(&mut css, &mut resolved, &inline);
            let resolved: Computeds = std::rc::Rc::new(resolved);
            let css: Styled = std::rc::Rc::new(css);
            apply_css_to_element(&mut el, &css);
            if css.visibility == Some(false) {
                self.app.hidden_count += 1;
                pushed_hidden = true;
            }
            if self.app.hidden_count > 0 {
                self.tree.set_flags(node, NodeFlags::empty());
            }
            // PROP1 `z-index`: applied once the cascade has resolved.
            // Overlay roots keep OVERLAY_Z — they route to the overlay pass
            // regardless, and a stylesheet must not be able to demote a
            // dropdown under the page.
            if !this_overlay {
                if let Some(z) = css.z_index {
                    self.tree.set_z(node, z.max(0) as u32);
                }
            }
            self.app.node_style.insert(node, css);
            self.app.node_computed.insert(node, resolved);
        }
        let pushed_desc = self.app.style_env.is_some();
        // B.2b: this node's own styles resolved against the *enclosing*
        // container (CSS semantics); its descendants query this one. Size
        // comes from the previous layout by build order (`None` until
        // measured — queries fail closed for that pass).
        let pushed_container = el.container;
        if pushed_container {
            let seq = self.app.container_nodes.len();
            self.app.container_nodes.push(node);
            self.app
                .container_stack
                .push(self.app.container_prev.get(seq).copied());
        }

        // Text nodes get a fixed size from measurement.
        // T2: captured before `el.style` is moved out below — these decide
        // whether a text node's box is observable, and therefore whether its
        // width may be left to the parent. See `deferred` below.
        let box_is_invisible =
            el.background.is_none() && el.border.is_none() && el.get_shadow().is_none() && !el.clip;
        let mut style = el.style;
        let (pl, pt) = (dim_px(style.padding.left), dim_px(style.padding.top));
        let (pr, pb) = (dim_px(style.padding.right), dim_px(style.padding.bottom));
        let pad = (pl, pt);
        let mut text_wrap: Option<f32> = None;
        let mut ellipsized: Option<String> = None;
        if let NodeContent::Text(txt, ts) = &el.content {
            // An explicit pixel width turns the label into a wrapping paragraph:
            // we lay out into the content box (width minus horizontal padding) and
            // keep that width, taking only the (wrapped) height from the block.
            // Otherwise the box is sized to the unwrapped text *plus* padding so
            // the label has room; it's then painted at the padded origin.
            let mut wrap = match style.width {
                Dim::Px(w) => Some((w - (pl + pr) as f32).max(0.0)),
                _ => None,
            };
            // PROP1 `text-wrap: nowrap`: keep the explicit width for the BOX but
            // shape unwrapped, so the run overflows on one line instead of
            // folding. Read back from `node_style` because `css` was moved there
            // above. Pair it with `overflow: hidden` to clip the overflow — this
            // property decides line breaking, not clipping.
            if self.app.node_style.get(&node).and_then(|s| s.text_wrap) == Some(false) {
                wrap = None;
            }
            // PROP1 `text-overflow: ellipsis`. Only meaningful with a bounded
            // width AND no wrapping — a wrapping paragraph has no overflowing
            // line to truncate, it just gets taller.
            if self.app.node_style.get(&node).and_then(|s| s.text_ellipsis) == Some(true)
                && wrap.is_none()
            {
                if let Dim::Px(w) = style.width {
                    let avail = (w - (pl + pr) as f32).max(0.0);
                    ellipsized = self.app.text.ellipsized_text(txt, ts, avail);
                }
            }
            // T2: a single unwrapped line whose width its parent assigns has
            // no intrinsic size anyone consumes — the height is font metrics
            // (T1) and the width is the parent's. Shaping it at layout time
            // computes a glyph advance that is then thrown away, once per node
            // per frame, which measured as 87% of a 10 000-row frame.
            //
            // Paint still shapes what it draws, so the run is shaped once for
            // the rows actually on screen instead of once for every row in the
            // list. That is what Qt and GTK do, and it is the whole gap.
            // The guard has two halves. The first is "does anyone consume this
            // node's intrinsic width" — a single unwrapped line whose parent
            // assigns its cross size. The second is "would anyone SEE the
            // difference": leaving the width `Auto` makes the box span the
            // parent instead of hugging the glyphs, which is what CSS
            // prescribes for a stretched block but is visible the moment
            // anything paints that box or positions text inside it.
            //
            // So a node is deferred only if its box is invisible: nothing fills
            // it, nothing outlines it, and the text sits at the start of it.
            // Without this second half the optimisation is a rendering change
            // wearing a performance change's clothes — it moved the `combobox`
            // doc shot, which is how it was caught.
            let deferred = self.stretched
                && self.cb_definite
                && style.width == Dim::Auto
                && wrap.is_none()
                && ellipsized.is_none()
                && box_is_invisible
                && ts.align == lumen_text::TextAlign::Start
                && !txt.contains('\n');
            if deferred {
                let lh = self.app.text.line_height_of(ts);
                if style.height == Dim::Auto {
                    style.height = Dim::px(lh.ceil() + (pt + pb) as f32);
                }
                // `style.width` deliberately left `Auto`: the parent's stretch
                // resolves it, so no glyph advance is needed.
                //
                // F3.5 × T2: the binding must be retained on THIS path too.
                // T2 landed after F3.5 and bypassed the eager sizing block
                // below, so `pending_text` fell through to the structural
                // safety net — every bound write under a definite containing
                // block became a full rebuild (MUT0: 18.4 ms vs 0.6 ms at
                // N=10 000). A deferred box takes nothing from the glyphs, so
                // the patch check is "still one line", not a measurement.
                if let Some((dynamic, deps)) = pending_text.take() {
                    self.app.text_bindings.push(BoundText {
                        node,
                        dynamic,
                        deps,
                        wrap,
                        auto_w: false,
                        auto_h: false,
                        w: 0.0,
                        h: 0.0,
                        patchable: true,
                        deferred: true,
                    });
                }
                text_wrap = wrap;
            } else {
                // W0404: this label had to be shaped to be laid out, because a
                // container above it sizes itself to its content and therefore
                // genuinely needs the glyph advance. Counted so the audit can
                // say so — the cost is otherwise invisible.
                if style.width == Dim::Auto && wrap.is_none() && self.stretched && !self.cb_definite
                {
                    self.app.shaped_for_indefinite += 1;
                }
                let block = self.app.text.shaped(txt, ts, wrap, ts.align);
                // Size the box to the glyphs ONLY when the author asked for nothing.
                // `== Dim::Auto` rather than `wrap.is_none()`, which clobbered two
                // widths the author *had* expressed:
                //
                //  * `Dim::Percent` — `width: 100%` on a label silently became the
                //    glyph width, in normal flow as well as absolute. That is what
                //    made a `VirtualList` of bare `text` rows shrink-wrap: nothing
                //    could stretch the row, so everything right of the label fell
                //    through it on a tap. The custom-leaf branch below already gets
                //    this right ("let an explicit width win so a leaf can flex/fill,
                //    e.g. a chart at `width: 100%`"); text was the odd one out.
                //  * `Dim::Px` under `text-wrap: nowrap` — which sets `wrap = None`
                //    precisely in order to *keep the explicit width for the box*, and
                //    then had it overwritten two lines later.
                //
                // A percentage cannot feed the wrap width: the containing block is
                // not resolved until layout runs, and this measurement happens
                // during the build. So a percentage-width label lays out as one
                // unwrapped line inside a stretched box — the same shape as
                // `nowrap`. Wrapping still needs a definite `Dim::px` (or a sized
                // container around the label).
                // F3.5: capture BEFORE the assignments below overwrite the dims —
                // an axis the author fixed cannot be moved by a new measurement.
                let auto_w = style.width == Dim::Auto;
                let auto_h = style.height == Dim::Auto;
                if auto_w {
                    style.width = Dim::px(block.width().ceil() + (pl + pr) as f32);
                }
                // Same guard, same reason as the width above: an explicit height was
                // being overwritten by the measured glyph height. A `VirtualList`
                // sets each item's height to `item_height`, so a 24 px-pitch list of
                // text rows laid out 21 px rows — a 3 px strip between every pair
                // that painted no background and took no taps. CSS gives a
                // fixed-height element that height with the text at the top, which
                // is what the paint already does (the run is drawn at the padded
                // origin), so honouring it needs nothing else.
                //
                // The "text ignores an explicit height" gotcha this retires cost one
                // golden across the whole corpus: `Grid`'s doc-shot, where the cells
                // now fill their 32 px rows instead of leaving a pale band between
                // them. That band was the same defect, sitting in a committed image.
                if auto_h {
                    style.height = Dim::px(block.height().ceil() + (pt + pb) as f32);
                }
                // F3.5: retain the binding with what this build measured, so a
                // later update can ask "would the box change?" without rebuilding.
                if let Some((dynamic, deps)) = pending_text.take() {
                    self.app.text_bindings.push(BoundText {
                        node,
                        dynamic,
                        deps,
                        wrap,
                        auto_w,
                        auto_h,
                        w: block.width().ceil(),
                        h: block.height().ceil(),
                        patchable: ellipsized.is_none(),
                        deferred: false,
                    });
                }
                text_wrap = wrap;
            }
        } else if let NodeContent::Custom(w) = &el.content {
            // Size a custom leaf from its intrinsic measure (E2), but let an
            // explicit `width`/`height` win so a leaf can flex/fill (e.g. a chart
            // at `width: 100%`). The measure sees the constrained axes as available
            // space; only an `Auto` axis is replaced by the intrinsic result.
            let avail = kurbo::Size::new(
                match style.width {
                    Dim::Px(v) => v as f64,
                    _ => f64::INFINITY,
                },
                match style.height {
                    Dim::Px(v) => v as f64,
                    _ => f64::INFINITY,
                },
            );
            let s = w.measure(avail);
            if matches!(style.width, Dim::Auto) {
                style.width = Dim::px(s.width.max(0.0) as f32);
            }
            if matches!(style.height, Dim::Auto) {
                style.height = Dim::px(s.height.max(0.0) as f32);
            }
        }

        // F3.5 safety net. The binding is only retained by the text sizing
        // block above, which is reachable because evaluating a `dyn_text`
        // rewrites `el.content` to `NodeContent::Text` unconditionally. If some
        // future path ever slips past it, the reads would be isolated AND
        // unretained — a change nothing would notice. Fall back to treating
        // them as structural, which is what they were before F3.5.
        if let Some((_, reads)) = pending_text.take() {
            debug_assert!(false, "text binding on a node that never measured text");
            self.app.structural_reads.extend(&reads);
        }

        // O0.14: lift the rare half out before the children are moved — it is
        // moved wholesale into `NodeMeta` below, and `el` is partially moved
        // from that point on.
        let el_rare = el.rare.take();
        // Consume the children (move, not clone) and recurse.
        let pushed_disabled = el.disabled;
        if pushed_disabled {
            self.app.disabled_count += 1;
        }
        // The children are produced now, while this node is open and on every
        // context stack — which is what makes context imposition work, and what
        // a `Vec<Element>` field could not express.
        // T2: what this node tells its children about their width.
        //
        // `definite` is CSS's definite/indefinite distinction: a length is
        // definite, a percentage only if its containing block is, and `Auto` only
        // if a parent stretches it. `gives_width` is whether this node assigns
        // its children's cross size — a flex column with the default stretch
        // alignment does; a row does not (width is its MAIN axis), and an
        // explicit non-stretch `align-items` opts out.
        let definite = match style.width {
            Dim::Px(_) => true,
            Dim::Percent(_) => self.cb_definite,
            Dim::Auto => self.stretched && self.cb_definite,
        };
        let gives_width = style.display == lumen_layout::Display::Flex
            && style.flex_direction == lumen_layout::FlexDirection::Column
            && matches!(style.align_items, None | Some(lumen_layout::Align::Stretch));
        let (outer_cb, outer_stretch) = (self.cb_definite, self.stretched);
        self.cb_definite = definite;
        self.stretched = gives_width;
        let child_lnodes: Vec<LayoutNode> = children(self, node, this_overlay, el.stacks_children);
        self.cb_definite = outer_cb;
        self.stretched = outer_stretch;
        if pushed_desc {
            self.app.pop_desc();
        }
        if pushed_container {
            self.app.container_stack.pop();
        }
        if pushed_hidden {
            self.app.hidden_count -= 1;
        }
        if pushed_disabled {
            self.app.disabled_count -= 1;
        }
        // O0.9: the post-css `LayoutStyle` used to be retained here, for
        // A.3.2's copy-forward path — a memo-hit span rebuilt its taffy nodes
        // and wanted the derived style back rather than re-deriving it. F2.2
        // replaced copy-forward with splice-in-place: a spliced span KEEPS its
        // taffy nodes, so nothing rebuilds them and nothing read the map. It
        // was still written for every node of every frame — a 598-byte clone
        // plus a hash insert, and a hash remove per freed node — with no
        // reader anywhere in the workspace.
        let lnode = if child_lnodes.is_empty() {
            self.layout.leaf(&style)
        } else {
            self.layout.container(&style, &child_lnodes)
        };
        // F2.2: remember which taffy node laid this one out. Recorded here,
        // at creation, rather than in a post-pass over a `built` vector —
        // spliced spans are never enumerated, so there is no such vector any
        // more, and the bounds pass reads this back off the arena instead.
        self.tree.set_lnode(node, lnode.raw());

        // Move the remaining fields into the retained NodeMeta (no clones).
        // O0.13: the rare half is allocated only when the node actually has
        // one of these. A label in a list has none of them, so it carries a
        // null pointer instead of 304 bytes of `None`.
        let rare = if el_rare.as_ref().is_some_and(|r| r.on_wheel.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_drag.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_drop.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_text.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_key.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_caret_set.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_dismiss.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_increment.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_decrement.is_some())
            || el_rare.as_ref().is_some_and(|r| r.on_set_value.is_some())
            || el_rare.as_ref().is_some_and(|r| r.caret_byte.is_some())
            || el_rare.as_ref().is_some_and(|r| r.selection.is_some())
            || el_rare.as_ref().is_some_and(|r| r.scroll.is_some())
            || el_rare.as_ref().is_some_and(|r| r.shadow.is_some())
            || el_rare.as_ref().is_some_and(|r| r.set_size.is_some())
            || el_rare.as_ref().is_some_and(|r| r.position_in_set.is_some())
        {
            // O0.14: `Element`'s rare half has the same shape as `NodeMeta`'s,
            // so the whole box moves across instead of fourteen field reads.
            let r = el_rare.unwrap_or_default();
            Some(Box::new(RareMeta {
                on_wheel: r.on_wheel,
                on_drag: r.on_drag,
                on_drop: r.on_drop,
                on_text: r.on_text,
                on_key: r.on_key,
                on_caret_set: r.on_caret_set,
                on_dismiss: r.on_dismiss,
                on_increment: r.on_increment,
                on_decrement: r.on_decrement,
                on_set_value: r.on_set_value,
                caret_byte: r.caret_byte,
                selection: r.selection,
                scroll: r.scroll,
                shadow: r.shadow,
                set_size: r.set_size,
                position_in_set: r.position_in_set,
            }))
        } else {
            None
        };
        self.meta.insert(
            node,
            NodeMeta {
                rare,
                id: el.id,
                role: el.role,
                label: el.label,
                value: el.value,
                classes: el.classes,
                actions: el.actions,
                states: el.states,
                focusable: el.focusable,
                autofocus: el.autofocus,
                elide: el.elide_semantics,
                #[cfg(feature = "dev-observability")]
                deps: node_deps,
                on_click: el.on_click,
                background: el.background,
                border: el.border,
                corner_radius: el.corner_radius,
                clip: el.clip,
                overlay: el.overlay,
                cursor: el.cursor,
                css_inline: el.css_inline.take(),
                content: el.content,
                pad,
                wrap_width: text_wrap,
                display_text: ellipsized,
            },
        );
        if let Some(key) = span_key {
            self.app.scope_spans.insert(
                key,
                SpanRec {
                    root: node,
                    count: (self.tree.len() - span_start) as u32,
                    ctx_hash: span_hash,
                    impure: self.app.impure_seen > impure_at,
                },
            );
        }
        (node, lnode)
    }
}

impl<R: lumen_render::Renderer, E: lumen_core::tasks::Spawner, P: PlatformConfig>
    Headless<R, E, P>
{
    // --- paint --------------------------------------------------------------

    fn build_display_list(&mut self) -> (DisplayList, Vec<lumen_render::TextTarget>) {
        let mut dl = DisplayList::new();
        let mut text_targets: Vec<lumen_render::TextTarget> = Vec::new();
        #[cfg(feature = "dev-observability")]
        self.node_ink.clear(); // repopulated per node as text runs are emitted
        self.node_caret.clear();
        #[cfg(feature = "dev-observability")]
        self.node_text_metrics.clear();
        // MUT2: refresh the bound nodes' footprints. A bound node that never
        // reaches emission (hidden subtree, visibility:none) simply has no
        // entry — it paints nothing, so a patch of it changes no pixels.
        self.dl_patch.clear();
        let bound: crate::fxhash::HashSet<NodeIndex> = self
            .text_bindings
            .iter()
            .map(|b| b.node)
            .chain(self.bg_bindings.iter().map(|b| b.node))
            .collect();
        // PROP1 `z-index`: siblings paint in ascending z. Sibling-scoped, so
        // the depth-keyed clip stack below still sees a strict preorder — a flat
        // z sort would not (see `Tree::paint_order`).
        let order = self.tree.paint_order();
        // Preorder depth of every node, and a partition into the main pass and the
        // overlay pass (nodes inside an `overlay` subtree). Overlays paint last so
        // they sit above the rest of the UI and escape ancestor clips (dropdown
        // menus, popovers, tooltips). Both subsets keep document order.
        let root = order.first().copied();
        let mut depth: HashMap<NodeIndex, u32> = HashMap::default();
        let mut main_order: Vec<NodeIndex> = Vec::new();
        let mut overlay_order: Vec<NodeIndex> = Vec::new();
        let mut overlay_depths: Vec<u32> = Vec::new();
        let mut hidden_depths: Vec<u32> = Vec::new();
        for node in order {
            let d = if Some(node) == root {
                0
            } else {
                depth.get(&self.tree.parent(node)).map_or(0, |p| p + 1)
            };
            depth.insert(node, d);
            while overlay_depths.last().is_some_and(|&od| d <= od) {
                overlay_depths.pop();
            }
            // B.3 visibility: a hidden subtree paints nothing (layout space
            // is kept — the partition just drops its nodes from both passes).
            while hidden_depths.last().is_some_and(|&hd| d <= hd) {
                hidden_depths.pop();
            }
            if !hidden_depths.is_empty() {
                continue;
            }
            if self
                .node_style
                .get(&node)
                .and_then(|s| s.visibility)
                .is_some_and(|v| !v)
            {
                hidden_depths.push(d);
                continue;
            }
            let is_root = self.meta.get(&node).is_some_and(|m| m.overlay);
            let inside = !overlay_depths.is_empty() || is_root;
            if is_root {
                overlay_depths.push(d);
            }
            if inside {
                overlay_order.push(node);
            } else {
                main_order.push(node);
            }
        }
        self.emit_pass(&main_order, &depth, &bound, &mut dl, &mut text_targets);
        self.emit_pass(&overlay_order, &depth, &bound, &mut dl, &mut text_targets);
        (dl, text_targets)
    }

    /// Emit draw commands for `order` (a document-ordered node subset), opening/
    /// closing `overflow:hidden` clip layers via a depth-keyed stack.
    fn emit_pass(
        &mut self,
        order: &[NodeIndex],
        depth: &HashMap<NodeIndex, u32>,
        bound: &crate::fxhash::HashSet<NodeIndex>,
        dl: &mut DisplayList,
        text_targets: &mut Vec<lumen_render::TextTarget>,
    ) {
        let mut clip_stack: Vec<u32> = Vec::new();
        for &node in order {
            let bounds = self.tree.bounds(node);
            let d = depth.get(&node).copied().unwrap_or(0);
            while clip_stack.last().is_some_and(|&cd| d <= cd) {
                dl.push(DrawCmd::PopLayer);
                clip_stack.pop();
            }
            let Some(m) = self.meta.get(&node) else {
                continue;
            };
            // `.lss` overrides the widget's hardcoded background/radius.
            let css = self.node_style.get(&node);
            // R.3: a node fully outside the canvas emits nothing (scrolled-
            // away content skips DL emission and raster). Nodes carrying
            // layer effects still run so descendants compose correctly.
            let offscreen = bounds.x1 <= 0.0
                || bounds.y1 <= 0.0
                || bounds.x0 >= self.size.width
                || bounds.y0 >= self.size.height;
            if offscreen
                && !m.clip
                && css.is_none_or(|s| {
                    s.clip.is_none() && s.opacity.unwrap_or(1.0) >= 1.0 && s.blend_mode.is_none()
                })
            {
                continue;
            }
            let mut bg = css.and_then(|s| s.background).or(m.background);
            // Hover feedback: lighten a dark control / darken a light one while
            // the pointer is over a clickable node. Automatic for every button.
            if let Some(c) = bg {
                if m.on_click.is_some() && self.tree.flags(node).contains(NodeFlags::HOVERED) {
                    bg = Some(hover_tint(c));
                }
            }
            let radius = css
                .and_then(|s| s.border_radius)
                .map(|r| r as f64)
                .unwrap_or(m.corner_radius);
            // B.3: 2–4-value `border-radius` — per-corner radii for the
            // fill/border/clip/backdrop shapes (the shadow sprite keeps the
            // uniform top-left fallback).
            let radii = css
                .and_then(|s| s.border_radius_corners)
                .map(|c| CornerRadii {
                    tl: c[0] as f64,
                    tr: c[1] as f64,
                    br: c[2] as f64,
                    bl: c[3] as f64,
                })
                .unwrap_or(CornerRadii::all(radius));
            // B.3: `.lss` opacity < 1 wraps the node's subtree in a
            // compositing layer — tracked on the same depth-keyed stack as
            // the clip layer, so it pops when the subtree ends.
            let opacity = css.and_then(|s| s.opacity).unwrap_or(1.0);
            // B.3: `.lss` blend-mode shares the compositing layer with opacity
            // — one PushLayer carries both when either is non-default.
            let blend = match css.and_then(|s| s.blend_mode) {
                Some(lumen_style::StyleBlend::Multiply) => BlendMode::Multiply,
                Some(lumen_style::StyleBlend::Screen) => BlendMode::Screen,
                Some(lumen_style::StyleBlend::Overlay) => BlendMode::Overlay,
                Some(lumen_style::StyleBlend::Darken) => BlendMode::Darken,
                Some(lumen_style::StyleBlend::Lighten) => BlendMode::Lighten,
                Some(lumen_style::StyleBlend::Normal) | None => BlendMode::SourceOver,
            };
            // PROP1 `transform`: `PushLayer` already carries an `Affine`
            // applied at composite time, and both backends honour it — so this
            // is a bridge, not a new render pass.
            //
            // The matrix is re-anchored about `transform-origin` (default the
            // node's centre, as in CSS) by translating the origin to 0,0,
            // applying, and translating back. Without that, `rotate` would
            // swing the node around the window's top-left.
            //
            // Folded into the SAME layer as opacity/blend when one is already
            // open, so a transformed translucent node costs one layer, not two.
            let node_transform = css
                .and_then(|s| s.transform)
                .filter(|t| *t != kurbo::Affine::IDENTITY);
            let composed = node_transform.map(|t| {
                let (ox, oy) = css.and_then(|s| s.transform_origin).unwrap_or((0.5, 0.5));
                let px = bounds.x0 + bounds.width() * ox;
                let py = bounds.y0 + bounds.height() * oy;
                kurbo::Affine::translate((px, py)) * t * kurbo::Affine::translate((-px, -py))
            });
            // PROP1 `filter: blur()` — blurs the node's OWN content, so like
            // opacity and blend it needs the subtree in a layer first.
            let node_blur = css.and_then(|s| s.filter_blur).unwrap_or(0.0);
            if opacity < 1.0
                || blend != BlendMode::SourceOver
                || composed.is_some()
                || node_blur > 0.0
            {
                dl.push(DrawCmd::PushLayer {
                    clip: None,
                    opacity: opacity.clamp(0.0, 1.0),
                    transform: composed.unwrap_or(kurbo::Affine::IDENTITY),
                    filter_blur: node_blur,
                    blend,
                });
                clip_stack.push(d);
            }
            // overflow:hidden — open a clip layer for this node's subtree (its own
            // fill + descendants paint into it, masked to its rounded bounds).
            // B.3: `.lss` clip overrides the element flag — `none` disables,
            // `bounds` squares off the corners, `rounded` follows the radius.
            let clip_mode = css.and_then(|s| s.clip);
            let clip_on = clip_mode
                .map(|c| c != lumen_style::StyleClip::None)
                .unwrap_or(m.clip);
            if clip_on {
                let clip_radii = match clip_mode {
                    Some(lumen_style::StyleClip::Bounds) => CornerRadii::all(0.0),
                    _ => radii,
                };
                dl.push(DrawCmd::PushLayer {
                    clip: Some(RoundedRect {
                        rect: bounds,
                        radii: clip_radii,
                    }),
                    opacity: 1.0,
                    transform: kurbo::Affine::IDENTITY,
                    filter_blur: 0.0,
                    blend: BlendMode::SourceOver,
                });
                clip_stack.push(d);
            }
            // Drop shadow: a soft penumbra — the shadow shape rasterized once and
            // Gaussian-blurred (the shared blur primitive). The sprite is static
            // for a given box, so cache it and blit each frame rather than
            // re-blurring (a large per-frame blur would dominate frame time).
            // B.3: `.lss` shadow overrides the widget's hardcoded one, like
            // background/radius above.
            let shadow = css
                .and_then(|s| s.shadow)
                .map(|ss| crate::element::Shadow {
                    dx: ss.dx as f64,
                    dy: ss.dy as f64,
                    blur: ss.blur as f64,
                    spread: ss.spread as f64,
                    color: ss.color,
                })
                .or(m.shadow().copied());
            if let Some(sh) = shadow {
                let w = bounds.width();
                let h = bounds.height();
                let margin = (sh.spread.max(0.0) + sh.blur).ceil() + 2.0;
                let [r, g, b, a] = sh.color.to_srgb8();
                // 9-slice: the sprite is sized by STYLE, not by the element.
                //
                // `blurred` runs three box passes of radius `blur` per axis, so
                // a pixel is influenced by anything within `3 * blur` of it —
                // note 3x, not 1x. Beyond `radius + spread + 3 * blur` from a
                // corner arc, the blurred edge stops changing along that edge,
                // so one strip of it repeats. Rasterizing a shorter synthetic
                // box and stretching that strip is therefore EXACT, not an
                // approximation, and nearest sampling keeps it exact because
                // every row of a constant strip is identical.
                //
                // Without this a 12 016 px card produced a 12 016 px sprite and
                // panicked in `create_texture`; it also blurred the whole area
                // every time the card resized, and thrashed the 64-entry cache
                // with one sprite per distinct size.
                // `+ 1` is defensive margin, not a fix for an observed failure:
                // the strip lands at exactly `margin + inv`, which is the last
                // index the corner arc can reach, and `shadow_slice.rs` is
                // byte-identical with or without it. It costs 2 px of sprite and
                // covers fractional radius/blur, where the reach does not land
                // on an integer.
                let inv = (radius + sh.spread).max(0.0) + 3.0 * sh.blur.max(0.0) + 1.0;
                let min_sliceable = 2.0 * inv + 1.0;
                // `LUMEN_NO_SHADOW_SLICE` forces the pre-slice path. It exists
                // for `shadow_slice.rs`, which byte-compares the two: no golden
                // in the suite has a shadowed element over the threshold, so
                // without an explicit equivalence test the sliced path would
                // ship completely unexercised.
                let no_slice = std::env::var_os("LUMEN_NO_SHADOW_SLICE").is_some();
                let w_syn = if w > min_sliceable && !no_slice {
                    min_sliceable
                } else {
                    w
                };
                let h_syn = if h > min_sliceable && !no_slice {
                    min_sliceable
                } else {
                    h
                };
                let key = (
                    (w_syn * 4.0).round() as i32,
                    (h_syn * 4.0).round() as i32,
                    (radius * 4.0).round() as i32,
                    (sh.blur * 4.0).round() as i32,
                    (sh.spread * 4.0).round() as i32,
                    u32::from_le_bytes([r, g, b, a]),
                );
                let sprite = if let Some(c) = self.shadow_cache.get(&key) {
                    c.clone()
                } else {
                    let sw = (w_syn + 2.0 * margin).ceil() as u32;
                    let sh_px = (h_syn + 2.0 * margin).ceil() as u32;
                    let mut sdl = DisplayList::new();
                    let base = Rect::new(margin, margin, margin + w_syn, margin + h_syn)
                        .inflate(sh.spread, sh.spread);
                    // Rasterize the solid shadow shape, then blur it into a soft
                    // penumbra. The margin reserves room for the blur to spread.
                    sdl.push(DrawCmd::Rect {
                        rect: base,
                        brush: Brush::Solid(Color::srgb8(r, g, b, a)),
                        radii: CornerRadii::all((radius + sh.spread).max(0.0)),
                        border: None,
                    });
                    let solid = cpu::render(&sdl, sw.max(1), sh_px.max(1), Color::TRANSPARENT);
                    let img = solid.blurred(sh.blur.round().max(0.0) as u32);
                    const CAP: usize = 64;
                    if self.shadow_cache.len() >= CAP {
                        // R.5: half-retention — sprites are expensive blurs.
                        let mut keep = self.shadow_cache.len() / 2;
                        self.shadow_cache.retain(|_, _| {
                            let k = keep > 0;
                            keep = keep.saturating_sub(1);
                            k
                        });
                    }
                    self.shadow_cache.insert(key, img.clone());
                    img
                };
                let sw_f = sprite.width() as f64;
                let sh_f = sprite.height() as f64;
                // Bands are expressed in DESTINATION space (element-sized) and
                // mapped back to the sprite by `split`.
                let iw = w + 2.0 * margin;
                let ih = h + 2.0 * margin;
                let id = lumen_render::ImageId(dl.images.len() as u32);
                dl.images.push(sprite);
                // Integer placement + nearest sampling makes each blit a straight
                // 1:1 copy (no resampling); a sub-pixel shadow shift is invisible.
                let px = (bounds.x0 + sh.dx - margin).round();
                let py = (bounds.y0 + sh.dy - margin).round();
                // The opaque box bg (drawn next) covers the box interior, so blit
                // only the surrounding penumbra: skip the largest rect provably
                // under the rounded bg (box inset by its radius) and emit the rest
                // as 4 bands. This is the frame's most expensive blit, and the
                // interior is ~half its pixels.
                let sx0 = (bounds.x0 - px + radius).ceil().clamp(0.0, iw);
                let sy0 = (bounds.y0 - py + radius).ceil().clamp(0.0, ih);
                let sx1 = (bounds.x0 - px + w - radius).floor().clamp(0.0, iw);
                let sy1 = (bounds.y0 - py + h - radius).floor().clamp(0.0, ih);
                // Destination extent of the shadow, which stays element-sized
                // even when the sprite was shrunk above.
                let dw = w + 2.0 * margin;
                let dh = h + 2.0 * margin;
                // Fixed border kept 1:1 on each side; whatever is between them
                // is one repeating strip. `+ 1.0` for the strip itself.
                let fixed_x = ((sw_f - 1.0) * 0.5).floor().max(0.0);
                let fixed_y = ((sh_f - 1.0) * 0.5).floor().max(0.0);
                // Split a destination span into pieces that each map to one
                // region of the sprite: the leading 1:1 border, the stretched
                // middle strip, and the trailing 1:1 border. When the sprite was
                // not shrunk on this axis (`dst_len == src_len`) this returns the
                // span unchanged, so unsliced shadows keep their exact old path.
                let split = |a: f64, b: f64, fixed: f64, dst_len: f64, src_len: f64| {
                    let mut out: Vec<(f64, f64, f64, f64)> = Vec::new();
                    if (dst_len - src_len).abs() < 0.5 {
                        out.push((a, b, a, b));
                        return out;
                    }
                    let tail = dst_len - fixed; // first dest coord of the trailing border
                    let lead = a.min(fixed).max(0.0);
                    if a < fixed {
                        out.push((a, b.min(fixed), a, b.min(fixed)));
                    }
                    let m0 = a.max(fixed);
                    let m1 = b.min(tail);
                    if m1 > m0 {
                        // One source pixel stretched across the middle. Exact:
                        // every row/column of that strip is identical.
                        out.push((m0, m1, fixed, fixed + 1.0));
                    }
                    if b > tail {
                        // Both ends map through the same 1:1 offset. Using
                        // `src_len` for the end instead assumes the band reaches
                        // the very bottom — the carve-out bands do not, and that
                        // silently COMPRESSED the trailing border into them.
                        let t0 = a.max(tail);
                        out.push((t0, b, src_len - (dst_len - t0), src_len - (dst_len - b)));
                    }
                    let _ = lead;
                    out
                };
                let mut band = |x0: f64, y0: f64, x1: f64, y1: f64| {
                    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                        return;
                    }
                    for (dx0, dx1, sx0, sx1) in split(x0, x1, fixed_x, dw, sw_f) {
                        for (dy0, dy1, sy0, sy1) in split(y0, y1, fixed_y, dh, sh_f) {
                            if dx1 - dx0 < 0.5 || dy1 - dy0 < 0.5 {
                                continue;
                            }
                            dl.push(DrawCmd::Image {
                                id,
                                src_rect: Rect::new(sx0, sy0, sx1, sy1),
                                dst_rect: Rect::new(px + dx0, py + dy0, px + dx1, py + dy1),
                                quality: lumen_render::Filter::Nearest,
                            });
                        }
                    }
                };
                if sx1 > sx0 && sy1 > sy0 {
                    band(0.0, 0.0, iw, sy0); // top
                    band(0.0, sy1, iw, ih); // bottom
                    band(0.0, sy0, sx0, sy1); // left
                    band(sx1, sy0, iw, sy1); // right
                } else {
                    band(0.0, 0.0, iw, ih); // box too small to carve a hole
                }
            }
            // Glass: blur the painted backdrop within this node's box before its
            // (translucent) fill goes on top. Emitted after the shadow so it
            // filters everything behind, but before bg/children.
            let blur = css.and_then(|s| s.backdrop_blur).unwrap_or(0.0);
            let refraction = css.and_then(|s| s.backdrop_refraction).unwrap_or(0.0);
            let specular = css.and_then(|s| s.backdrop_specular).unwrap_or(0.0);
            let saturate = css.and_then(|s| s.backdrop_saturate).unwrap_or(1.0);
            if blur > 0.0 || refraction > 0.0 || specular > 0.0 || saturate != 1.0 {
                dl.push(DrawCmd::BackdropFilter {
                    rect: bounds,
                    radii,
                    blur,
                    saturate,
                    refraction,
                    specular,
                });
            }
            // A focused text editor gets an accent focus ring (drawn on the box
            // edge). It's the *default* — an explicit border (element or `.lss`)
            // wins; customize focus feedback via a `&:focused { border: … }` rule.
            let focused = self.tree.flags(node).contains(NodeFlags::FOCUSED);
            let focus_border = (focused && m.on_caret_set().is_some()).then(|| Border {
                width: 2.0,
                color: crate::element::accent_color(),
            });
            // `.lss` border (shorthand or longhands) wins over an element border,
            // which wins over the focus ring.
            let css_border = css.and_then(|s| match (s.border_width, s.border_color) {
                (None, None) => None,
                (w, c) => Some(Border {
                    width: w.unwrap_or(1.0) as f64,
                    color: c.unwrap_or(Color::srgb8(0, 0, 0, 0xff)),
                }),
            });
            let border = css_border.or(m.border).or(focus_border);
            // Emit the box rect for a fill *or* a border (an outline-only box has
            // a transparent fill); nodes with neither stay rect-free as before.
            // B.3: `.lss` gradient backgrounds — box-relative geometry maps
            // onto the renderer's absolute-point brush here, where bounds are
            // known. A gradient beats the solid color; hover feedback tints
            // its stops the same way `hover_tint` treats a solid, so gradient
            // buttons don't read as inert.
            let gradient = css.and_then(|s| s.background_gradient.as_ref()).map(|g| {
                let mut brush = gradient_brush(g, bounds);
                if m.on_click.is_some() && self.tree.flags(node).contains(NodeFlags::HOVERED) {
                    hover_tint_brush(&mut brush);
                }
                brush
            });
            if bg.is_some() || border.is_some() || gradient.is_some() {
                if bound.contains(&node) {
                    self.dl_patch.entry(node).or_default().bg_cmd = Some(dl.cmds.len() as u32);
                }
                dl.push(DrawCmd::Rect {
                    rect: bounds,
                    brush: gradient.unwrap_or(Brush::Solid(bg.unwrap_or(Color::srgb8(0, 0, 0, 0)))),
                    radii,
                    border,
                });
            }
            // B.3: per-side borders — straight strips on top of the box
            // fill. Each strip is inset by the box's corner radii so it stops
            // where the corner arc begins (a full-width strip on a rounded
            // box reads as a line overshooting the corners).
            if let Some(sides) = css.map(|s| s.border_sides) {
                for (i, sb) in sides.iter().enumerate() {
                    let Some(sb) = sb else { continue };
                    let w = sb.width as f64;
                    let r = match i {
                        0 => Rect::new(
                            bounds.x0 + radii.tl,
                            bounds.y0,
                            bounds.x1 - radii.tr,
                            bounds.y0 + w,
                        ),
                        1 => Rect::new(
                            bounds.x1 - w,
                            bounds.y0 + radii.tr,
                            bounds.x1,
                            bounds.y1 - radii.br,
                        ),
                        2 => Rect::new(
                            bounds.x0 + radii.bl,
                            bounds.y1 - w,
                            bounds.x1 - radii.br,
                            bounds.y1,
                        ),
                        _ => Rect::new(
                            bounds.x0,
                            bounds.y0 + radii.tl,
                            bounds.x0 + w,
                            bounds.y1 - radii.bl,
                        ),
                    };
                    if r.width() <= 0.0 || r.height() <= 0.0 {
                        continue;
                    }
                    dl.push(DrawCmd::Rect {
                        rect: r,
                        brush: Brush::Solid(sb.color),
                        radii: CornerRadii::all(0.0),
                        border: None,
                    });
                }
            }
            // Immediate-mode canvas: draw in node-local coords offset to bounds.
            if let NodeContent::Canvas(draw) = &m.content {
                let mut frame = lumen_render::canvas::Frame::new(kurbo::Affine::translate((
                    bounds.x0, bounds.y0,
                )));
                draw(
                    &mut frame,
                    kurbo::Size::new(bounds.width(), bounds.height()),
                );
                let (cmds, texts) = frame.into_parts();
                for cmd in cmds {
                    dl.push(cmd);
                }
                for t in texts {
                    Self::rasterize_frame_text(&mut self.text, &mut self.text_cache, dl, t);
                }
            }
            if let NodeContent::Custom(w) = &m.content {
                // Paint a custom leaf via the same node-local Frame as Canvas (E2).
                let mut frame = lumen_render::canvas::Frame::new(kurbo::Affine::translate((
                    bounds.x0, bounds.y0,
                )));
                w.paint(
                    &mut frame,
                    kurbo::Size::new(bounds.width(), bounds.height()),
                );
                let (cmds, texts) = frame.into_parts();
                for cmd in cmds {
                    dl.push(cmd);
                }
                for t in texts {
                    Self::rasterize_frame_text(&mut self.text, &mut self.text_cache, dl, t);
                }
            }
            if let NodeContent::Image(img) = &m.content {
                let iw = img.width() as f64;
                let ih = img.height() as f64;
                let id = lumen_render::ImageId(dl.images.len() as u32);
                dl.images.push(img.clone());
                dl.push(DrawCmd::Image {
                    id,
                    src_rect: Rect::new(0.0, 0.0, iw, ih),
                    dst_rect: bounds,
                    quality: lumen_render::Filter::Nearest,
                });
            }
            if let NodeContent::Text(txt, ts) = &m.content {
                // PROP1 `text-overflow: ellipsis`: PAINT the truncated string,
                // while `m.content`'s text — and therefore the semantic tree the
                // agent and assistive tech read — stays the full one. This one
                // binding is the whole feature; everything else it touches is
                // deliberately left alone.
                let txt: &str = m.display_text.as_deref().unwrap_or(txt);
                // Apply a `.lss` text colour to the glyphs (the cascade also
                // drives background/radius above). Colour is size-neutral, so it
                // doesn't desync the layout box measured at build time; `.lss`
                // font-size/weight on text remain follow-on (they'd need the
                // measure pass to consult the cascade too).
                let mut ts = ts.clone();
                if let Some(c) = css.and_then(|s| s.color) {
                    ts.color = c;
                }
                // Text color is reused after `ts` is moved into layout (caret /
                // run brush / analysis target); capture it (Color is Copy).
                let text_color = ts.color;
                // Caret-follow horizontal scroll: a clipped editor has no scroll
                // offset of its own, so a caret past the box width was clipped out
                // of view (e.g. a masked password field once you type past its
                // width). Shift the text left just enough to keep the caret inside
                // the content box. Baked into `tx`, so glyphs, selection, and caret
                // move together. Gated on `clip_on` (an unclipped field already
                // shows the caret past its edge); genuinely wrapped content keeps
                // `caret_x <= avail`, so this is a no-op there.
                let scroll_x = match m.caret_byte() {
                    Some(caret) if focused && clip_on => {
                        let avail = (bounds.width() - 2.0 * m.pad.0).max(0.0);
                        let caret_x = self
                            .text
                            .shaped(txt, &ts, m.wrap_width, ts.align)
                            .caret_pos(caret)
                            .0 as f64;
                        (caret_x - avail).max(0.0)
                    }
                    _ => 0.0,
                };
                // Paint at the padded (content-box) origin, minus any caret
                // scroll, so a label sits inside its padding (centred for
                // symmetric padding) rather than jammed into the border-box
                // corner. Plain text has no padding/scroll, so this is a no-op.
                let tx = bounds.x0 + m.pad.0 - scroll_x;
                let ty = bounds.y0 + m.pad.1;
                // R3.4: emit a glyph run (positioned glyphs + atlas-bound coverage
                // bitmaps from the per-glyph cache) instead of a whole-string
                // sprite, so the GPU batches text through the atlas and a 1-char
                // edit re-rasterizes ≤1 glyph. `block` also drives the caret /
                // selection geometry below (same layout).
                let scale = self.scale as f32;
                // R5: reuse the cached **origin-relative** glyph run — translate to
                // (tx, ty) and intern its glyphs (cloning only ones new to this
                // frame). Skips the per-frame `glyph_run` rebuild (the dominant
                // display-list-emission cost) byte-identically — the pen rounds
                // before the origin is added, so translation commutes. Ink +
                // metrics come from the cache.
                let (run, run_rect, metrics) = {
                    let cached = self
                        .text
                        .shaped_run(txt, &ts, m.wrap_width, ts.align, scale);
                    let mut run = cached.run.clone();
                    for g in &mut run.glyphs {
                        g.x += tx as f32;
                        g.y += ty as f32;
                        g.image = dl.intern_glyph_ref(&cached.images[g.image as usize]);
                    }
                    let run_rect = Rect::new(
                        cached.ink[0] as f64 + tx,
                        cached.ink[1] as f64 + ty,
                        cached.ink[2] as f64 + tx,
                        cached.ink[3] as f64 + ty,
                    );
                    (run, run_rect, cached.metrics)
                };
                // Selection highlight (behind the glyphs) for a focused editor —
                // re-shape (cached, cheap) for the selection geometry.
                if focused && m.caret_byte().is_some() {
                    if let Some((a, b)) = m.selection().filter(|(a, b)| a != b) {
                        // PROP1: `.lss` `selection-color` overrides the
                        // built-in tint. The default keeps its own alpha
                        // (0x55) because the highlight paints BEHIND the
                        // glyphs and an opaque default would hide them; an
                        // author who sets an opaque colour has chosen that.
                        let sel = css
                            .and_then(|s| s.selection_color)
                            .unwrap_or_else(|| Color::srgb8(0x1a, 0x73, 0xe8, 0x55));
                        let block = self.text.shaped(txt, &ts, m.wrap_width, ts.align);
                        for (x0, y0, x1, y1) in block.selection_rects(a, b) {
                            dl.push(DrawCmd::Rect {
                                rect: Rect::new(
                                    tx + x0 as f64,
                                    ty + y0 as f64,
                                    tx + x1 as f64,
                                    ty + y1 as f64,
                                ),
                                brush: Brush::Solid(sel),
                                radii: CornerRadii::all(0.0),
                                border: None,
                            });
                        }
                    }
                }
                // Record the glyph-ink bounds for this node so the clipping audit
                // (W0104) and ui.getLayout can compare ink vs the layout box.
                #[cfg(feature = "dev-observability")]
                {
                    self.node_ink.insert(node, run_rect);
                    self.node_text_metrics.insert(node, metrics);
                }
                let run_id = dl.add_run(run);
                if bound.contains(&node) {
                    let e = self.dl_patch.entry(node).or_default();
                    e.text_cmd = Some(dl.cmds.len() as u32);
                    // An ellipsized display string, an editor caret (which also
                    // brings selection rects and caret-follow scroll), or a
                    // decoration rect below make the footprint more than the
                    // run — those frames take the full `paint()`.
                    e.ineligible |= m.display_text.is_some()
                        || m.caret_byte().is_some()
                        || css
                            .and_then(|s| s.text_decoration)
                            .unwrap_or(lumen_core::TextDecoration::None)
                            != lumen_core::TextDecoration::None;
                }
                dl.push(DrawCmd::GlyphRun {
                    run: run_id,
                    brush: Brush::Solid(text_color),
                    rect: run_rect,
                });
                // PROP1 `text-decoration`: a filled rect, drawn AFTER the run so
                // it sits over the glyphs — which is what a strike-through must
                // do, and is harmless for an underline (they do not overlap).
                //
                // Geometry comes from the font's own metrics rather than a
                // fraction of the box: the underline sits just below the
                // baseline (ascent from the top), and the strike at the middle
                // of the x-height, so both track font size and line height
                // instead of drifting as either changes.
                let decoration = css
                    .and_then(|s| s.text_decoration)
                    .unwrap_or(lumen_core::TextDecoration::None);
                if decoration != lumen_core::TextDecoration::None && run_rect.width() > 0.0 {
                    // ~7% of font size, floored at one physical pixel so the
                    // line never vanishes at small sizes or low scale.
                    let thickness = (ts.font_size as f64 * 0.07).max(1.0 / scale as f64);
                    let baseline = ty + metrics.ascent as f64;
                    let y = match decoration {
                        lumen_core::TextDecoration::Underline => baseline + thickness,
                        // Half the ascent approximates the x-height midpoint
                        // without needing an OS/2 table read.
                        _ => baseline - metrics.ascent as f64 * 0.5,
                    };
                    dl.push(DrawCmd::Rect {
                        rect: Rect::new(run_rect.x0, y, run_rect.x1, y + thickness),
                        brush: Brush::Solid(text_color),
                        radii: CornerRadii::all(0.0),
                        border: None,
                    });
                }
                // Caret (in front) for a focused editor — re-shape (cached) for
                // the caret geometry.
                if let Some(caret) = m.caret_byte().filter(|_| focused) {
                    let block = self.text.shaped(txt, &ts, m.wrap_width, ts.align);
                    let (cx, cy, ch) = block.caret_pos(caret);
                    let w = 1.5;
                    let cr = Rect::new(
                        tx + cx as f64,
                        ty + cy as f64,
                        tx + cx as f64 + w,
                        ty + cy as f64 + ch as f64,
                    );
                    self.node_caret.insert(node, cr);
                    dl.push(DrawCmd::Rect {
                        rect: cr,
                        brush: Brush::Solid(text_color),
                        radii: CornerRadii::all(0.0),
                        border: None,
                    });
                }
                // Mirror the painted text as a design-analysis target: the
                // foreground is the text's resolved color, the region its bounds.
                text_targets.push(lumen_render::TextTarget {
                    // Raw arena index here: display-list building runs during
                    // paint, before this frame's semantic tree exists, so the
                    // handle isn't derivable yet. `contrast_report` translates
                    // it before the value reaches an agent.
                    node: Some(node.index().to_string()),
                    label: Some(txt.to_string()),
                    foreground: text_color,
                    region: bounds,
                });
            }
        }
        // Close any clip layers still open at the end of the pass.
        for _ in 0..clip_stack.len() {
            dl.push(DrawCmd::PopLayer);
        }
    }

    /// Rasterize a [`FrameText`] (from a canvas / custom-leaf `fill_text`) into an
    /// image blit on `dl`, anchored per its opts. A free fn over the two fields it
    /// needs (not `&mut self`) so it composes with the `&self.meta` borrow held by
    /// the paint loop. Shares the glyph cache with own-text painting.
    fn rasterize_frame_text(
        text: &mut P::Text,
        cache: &mut HashMap<(String, u32, u32, u32, u32), RgbaImage>,
        dl: &mut DisplayList,
        t: lumen_render::canvas::FrameText,
    ) {
        use lumen_render::canvas::{AnchorX, AnchorY};
        let ts = lumen_text::TextStyle {
            font_size: t.opts.size,
            weight: t.opts.weight,
            color: t.opts.color,
            line_height: None,
            letter_spacing: 0.0,
            family: None,
            features: None,
            variations: None,
            italic: false,
            align: lumen_text::TextAlign::Start,
        };
        let [cr, cg, cb, ca] = ts.color.to_srgb8();
        let key = (
            t.text.clone(),
            ts.font_size.to_bits(),
            ts.weight.to_bits(),
            u32::from_le_bytes([cr, cg, cb, ca]),
            0, // no wrap
        );
        let img = if let Some(cached) = cache.get(&key) {
            cached.clone()
        } else {
            let align = ts.align;
            let block = text.layout(&t.text, ts, &[], None, align);
            let img = block.render(0, 0, Color::srgb8(255, 255, 255, 0));
            const CAP: usize = 512;
            if cache.len() >= CAP {
                cache.clear();
            }
            cache.insert(key, img.clone());
            img
        };
        let iw = img.width() as f64;
        let ih = img.height() as f64;
        // Offset the anchor point to the box's top-left, then snap to whole px so
        // the nearest-sampled blit stays crisp.
        let dx = match t.opts.anchor_x {
            AnchorX::Start => 0.0,
            AnchorX::Center => -iw / 2.0,
            AnchorX::End => -iw,
        };
        let dy = match t.opts.anchor_y {
            AnchorY::Top => 0.0,
            AnchorY::Middle => -ih / 2.0,
            AnchorY::Bottom => -ih,
        };
        let x = (t.pos.x + dx).round();
        let y = (t.pos.y + dy).round();
        let id = lumen_render::ImageId(dl.images.len() as u32);
        dl.images.push(img);
        dl.push(DrawCmd::Image {
            id,
            src_rect: Rect::new(0.0, 0.0, iw, ih),
            dst_rect: Rect::new(x, y, x + iw, y + ih),
            quality: lumen_render::Filter::Nearest,
        });
    }

    /// Rasterize the current build into `self.frame`, repainting only what
    /// changed since the last frame (R2). Returns the [`Damage`] applied.
    ///
    /// The retained `self.frame` always equals a full render: when nothing
    /// changed it is reused; when a sub-region changed, only that region is
    /// re-rendered (byte-identical to a full render there — R0
    /// `damage_equivalence`) and composited in, leaving the unchanged pixels
    /// (which still match) intact.
    /// MUT2: repaint a patch frame by rewriting the patched nodes' commands
    /// inside the retained display list, instead of rebuilding and diffing
    /// the whole list (`paint()` — two O(live nodes) passes). Damage is the
    /// union of each rewritten command's paint bounds before and after —
    /// exactly the rect `damage_between` would have found for the same
    /// change. Returns `None` when the frame can't be patched in place — no
    /// retained list, a stale CPU frame, a node whose footprint is more than
    /// its own command (`DlSlot::ineligible`), or a bg patch on a node that
    /// emitted no box — and the caller falls back to a full `paint()`, which
    /// is always correct. A partial rewrite before a bail is harmless for the
    /// same reason: the fallback rebuilds the list from scratch.
    fn paint_patched(
        &mut self,
        text_nodes: &[NodeIndex],
        bg_nodes: &[NodeIndex],
    ) -> Option<Damage> {
        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
        if self.last_dl.is_none()
            || (!self.surface_attached && (self.frame.width() != pw || self.frame.height() != ph))
        {
            return None;
        }
        let mut dl = self.last_dl.take().expect("checked above");
        let mut acc: Option<Rect> = None;
        fn union(acc: &mut Option<Rect>, b: Option<Rect>) {
            if let Some(b) = b {
                *acc = Some(acc.map_or(b, |r: Rect| r.union(b)));
            }
        }
        for &node in text_nodes {
            // Absent ⇒ the node painted nothing (hidden subtree) — the patch
            // changes no pixels, and the next full build will pick it up.
            let Some(slot) = self.dl_patch.get(&node).copied() else {
                continue;
            };
            let (false, Some(ci)) = (slot.ineligible, slot.text_cmd) else {
                self.last_dl = Some(dl);
                return None;
            };
            let ci = ci as usize;
            let Some(m) = self.meta.get(&node) else {
                self.last_dl = Some(dl);
                return None;
            };
            let NodeContent::Text(txt, ts) = &m.content else {
                self.last_dl = Some(dl);
                return None;
            };
            // Mirror the emission path: `.lss` color over the widget's own.
            let mut ts = ts.clone();
            if let Some(c) = self.node_style.get(&node).and_then(|s| s.color) {
                ts.color = c;
            }
            let bounds = self.tree.bounds(node);
            // No caret ⇒ no caret-follow scroll (ineligible covers editors).
            let tx = bounds.x0 + m.pad.0;
            let ty = bounds.y0 + m.pad.1;
            let scale = self.scale as f32;
            let (run, run_rect, _metrics) = {
                let cached = self.text.shaped_run(txt, &ts, m.wrap_width, ts.align, scale);
                let mut run = cached.run.clone();
                for g in &mut run.glyphs {
                    g.x += tx as f32;
                    g.y += ty as f32;
                    g.image = dl.intern_glyph_ref(&cached.images[g.image as usize]);
                }
                let run_rect = Rect::new(
                    cached.ink[0] as f64 + tx,
                    cached.ink[1] as f64 + ty,
                    cached.ink[2] as f64 + tx,
                    cached.ink[3] as f64 + ty,
                );
                (run, run_rect, cached.metrics)
            };
            let DrawCmd::GlyphRun {
                run: rid,
                rect: old_rect,
                ..
            } = dl.cmds[ci]
            else {
                self.last_dl = Some(dl);
                return None;
            };
            union(&mut acc, Some(old_rect.inflate(1.0, 1.0)));
            dl.runs[rid.0 as usize] = run;
            dl.cmds[ci] = DrawCmd::GlyphRun {
                run: rid,
                brush: Brush::Solid(ts.color),
                rect: run_rect,
            };
            union(&mut acc, Some(run_rect.inflate(1.0, 1.0)));
            #[cfg(feature = "dev-observability")]
            {
                self.node_ink.insert(node, run_rect);
                self.node_text_metrics.insert(node, _metrics);
            }
        }
        for &node in bg_nodes {
            let Some(slot) = self.dl_patch.get(&node).copied() else {
                continue;
            };
            let Some(ci) = slot.bg_cmd else {
                // The build emitted no box for this node (it had no fill,
                // border, or gradient then) — a patch that adds one needs a
                // new command, which only a full paint can place.
                self.last_dl = Some(dl);
                return None;
            };
            let ci = ci as usize;
            let Some(m) = self.meta.get(&node) else {
                self.last_dl = Some(dl);
                return None;
            };
            // Mirror the emission path's brush resolution, hover tint and all.
            let css = self.node_style.get(&node);
            let mut bg = css.and_then(|s| s.background).or(m.background);
            let hovered =
                m.on_click.is_some() && self.tree.flags(node).contains(NodeFlags::HOVERED);
            if let Some(c) = bg {
                if hovered {
                    bg = Some(hover_tint(c));
                }
            }
            let bounds = self.tree.bounds(node);
            let gradient = css.and_then(|s| s.background_gradient.as_ref()).map(|g| {
                let mut brush = gradient_brush(g, bounds);
                if hovered {
                    hover_tint_brush(&mut brush);
                }
                brush
            });
            let brush =
                gradient.unwrap_or(Brush::Solid(bg.unwrap_or(Color::srgb8(0, 0, 0, 0))));
            let bounds_now = dl.cmds[ci].paint_bounds();
            let DrawCmd::Rect { brush: b, .. } = &mut dl.cmds[ci] else {
                self.last_dl = Some(dl);
                return None;
            };
            *b = brush;
            union(&mut acc, bounds_now);
        }
        let damage = match acc {
            Some(r) => Damage::Region(r),
            None => Damage::None,
        };
        // The render tail, exactly as `paint()` does it for a region.
        if !self.surface_attached {
            if let Damage::Region(r) = damage {
                let bg = Color::srgb8(255, 255, 255, 255);
                let dirty = kurbo::Rect::new(
                    (r.x0 * self.scale).floor().max(0.0),
                    (r.y0 * self.scale).floor().max(0.0),
                    (r.x1 * self.scale).ceil().min(pw as f64),
                    (r.y1 * self.scale).ceil().min(ph as f64),
                );
                if dirty.width() >= 1.0 && dirty.height() >= 1.0 {
                    let tile = self
                        .renderer
                        .render_damage(&dl, pw, ph, self.scale, bg, dirty);
                    self.frame
                        .overwrite_rect(dirty.x0 as u32, dirty.y0 as u32, &tile);
                }
            }
        }
        self.last_dl = Some(dl);
        Some(damage)
    }

    fn paint(&mut self) -> Damage {
        let (dl, _) = self.build_display_list();
        // Layout/display list are in logical px; rasterize at physical px so the
        // frame matches a HiDPI surface 1:1 (no upscaling blur). scale 1.0 is
        // byte-identical to the unscaled path (goldens unaffected).
        let pw = (self.size.width * self.scale).round().max(1.0) as u32;
        let ph = (self.size.height * self.scale).round().max(1.0) as u32;
        let bg = Color::srgb8(255, 255, 255, 255);

        // Incremental only when we have a previous display list to diff against.
        // The CPU path additionally needs the retained frame to match the target
        // size; the surface path keeps no CPU frame, so the prev-dl is enough.
        let can_incremental = self.last_dl.is_some()
            && (self.surface_attached || (self.frame.width() == pw && self.frame.height() == ph));
        let damage = if can_incremental {
            lumen_render::damage_between(self.last_dl.as_ref().unwrap(), &dl)
        } else {
            Damage::Full
        };

        if self.surface_attached {
            // Direct-to-surface (1c): no CPU rasterization. The shell presents
            // the retained `last_dl` via `present_to_surface` when
            // `damage != None`, which reads `last_damage` (set below) and passes
            // the region down — R6.2 culls the list to it and R6.3 scissors the
            // redraw. Before that, granularity was computed here and discarded.
        } else {
            match damage {
                Damage::None => { /* nothing changed — reuse self.frame */ }
                Damage::Region(r) => {
                    // Logical → physical, integer-aligned, clamped to the frame.
                    let dirty = kurbo::Rect::new(
                        (r.x0 * self.scale).floor().max(0.0),
                        (r.y0 * self.scale).floor().max(0.0),
                        (r.x1 * self.scale).ceil().min(pw as f64),
                        (r.y1 * self.scale).ceil().min(ph as f64),
                    );
                    if dirty.width() >= 1.0 && dirty.height() >= 1.0 {
                        let tile = self
                            .renderer
                            .render_damage(&dl, pw, ph, self.scale, bg, dirty);
                        self.frame
                            .overwrite_rect(dirty.x0 as u32, dirty.y0 as u32, &tile);
                    }
                }
                Damage::Full => {
                    self.frame = self.renderer.render_frame(&dl, pw, ph, self.scale, bg);
                }
            }
        }
        self.last_dl = Some(dl);
        damage
    }

    /// Replace the frame renderer with another of the *same* type `R`, then
    /// re-render. (The backend type is chosen at construction via
    /// `App::with_renderer`; this only swaps the instance — e.g. a reconfigured
    /// `Box<dyn Renderer>` when `R` is the boxed escape-hatch type.)
    pub fn set_renderer(&mut self, renderer: R) {
        self.renderer = renderer;
        self.pump();
    }

    /// The active renderer backend's name (e.g. `"cpu"`).
    pub fn renderer_name(&self) -> &'static str {
        self.renderer.name()
    }

    /// Whether the active renderer is GPU-backed (O2.5 session fact).
    pub fn is_gpu(&self) -> bool {
        self.renderer.is_gpu()
    }

    /// The active graphics backend (`"Vulkan"`, `"Gl"`, `"cpu"`, …).
    pub fn backend(&self) -> &'static str {
        self.renderer.backend()
    }

    /// Shared reference to the background-work executor `E`. Lets a test reach a
    /// [`ManualSpawner`](lumen_core::tasks::ManualSpawner) after it has been moved
    /// into the runtime (to `run_pending` between pumps).
    pub fn executor(&self) -> &E {
        &self.executor
    }

    /// Set the host waker invoked when a background result is queued (the shell
    /// wires an event-loop wake so results schedule a frame). Headless leaves it
    /// unset; the next manual `pump` drains the deferred-op queue regardless.
    pub fn set_waker(&mut self, waker: lumen_core::tasks::WakeFn) {
        self.task_waker = Some(waker);
    }

    /// A deterministic APCA text-contrast report over the current frame's
    /// display list (prototype design-analysis surface, ADR pending). Each
    /// finding is bound to the `node-<index>` id of the text node it assesses,
    /// and contrast is measured against the *composited* backdrop.
    pub fn contrast_report(&mut self) -> lumen_render::ContrastReport {
        let (dl, mut targets) = self.build_display_list();
        // Text that a clipping ancestor cuts away entirely is not painted, so
        // it has no contrast to assess. A virtualized surface always has some:
        // every windowing widget materializes a row or two of overscan outside
        // its viewport so scrolling has something to reveal. Reporting those as
        // unreadable is a finding about pixels that do not exist.
        targets.retain(|t| {
            !t.node
                .as_deref()
                .and_then(|s| s.parse::<u32>().ok())
                .is_some_and(|i| self.is_clipped_away(i))
        });
        // ID1: translate arena indices into agent handles, so a finding binds
        // to an id the agent can actually use as a selector. Done here rather
        // than at paint time because the semantic tree only exists afterwards.
        for t in &mut targets {
            if let Some(idx) = t.node.as_deref().and_then(|s| s.parse::<u32>().ok()) {
                t.node = self.handle_for_index(idx).map(|h| h.to_wire());
            }
        }
        lumen_render::analyze_contrast(&dl, Color::srgb8(255, 255, 255, 255), &targets)
    }

    /// Whether the node at arena `index` falls entirely outside a clipping
    /// ancestor's box — i.e. nothing of it is painted.
    fn is_clipped_away(&self, index: u32) -> bool {
        let Some(node) = self.meta.keys().copied().find(|n| n.index() == index) else {
            return false;
        };
        let b = self.tree.bounds(node);
        let mut p = self.tree.parent(node);
        while p.is_some() {
            if self.meta.get(&p).is_some_and(|m| m.clip) {
                let cb = self.tree.bounds(p);
                if b.y1 <= cb.y0 || b.y0 >= cb.y1 || b.x1 <= cb.x0 || b.x0 >= cb.x1 {
                    return true;
                }
            }
            p = self.tree.parent(p);
        }
        false
    }

    /// ID1: arena index -> agent handle, via the semantic tree that holds both.
    fn handle_for_index(&self, index: u32) -> Option<lumen_core::identity::NodeHandle> {
        self.handle_index_map().get(&index).copied()
    }

    /// O0.4: the index → handle map for the current semantic tree, built once.
    ///
    /// One walk per tree instead of one walk per lookup. See `handle_index`.
    fn handle_index_map(&self) -> Rc<HashMap<u32, lumen_core::identity::NodeHandle>> {
        if let Some(m) = self.handle_index.borrow().as_ref() {
            return Rc::clone(m);
        }
        fn walk(
            n: &lumen_core::semantics::SemanticsNode,
            out: &mut HashMap<u32, lumen_core::identity::NodeHandle>,
        ) {
            out.insert(n.index, n.node);
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut map = HashMap::default();
        walk(&self.sem_root(), &mut map);
        let built = Rc::new(map);
        *self.handle_index.borrow_mut() = Some(Rc::clone(&built));
        built
    }

    // --- semantics ----------------------------------------------------------

    fn build_semantics(&self, node: NodeIndex) -> SemanticsNode {
        let role = self
            .meta
            .get(&node)
            .map(|m| m.role)
            .unwrap_or(Role::Generic);
        self.build_semantics_at(node, lumen_core::identity::NodeHandle::root(role.as_str()))
    }

    /// ID1: build a node's semantics under an already-derived handle.
    ///
    /// The handle is threaded down rather than recomputed per node because it
    /// folds the *path*: a child's identity is `fold(parent, local)`, so the
    /// parent's value has to be in hand. `local` is the author id when one is
    /// set — which is what lets a node keep its identity when a sibling is
    /// inserted above it, something `node-<index>` never survived — and the
    /// `(role, ordinal)` pair otherwise.
    fn build_semantics_at(
        &self,
        node: NodeIndex,
        handle: lumen_core::identity::NodeHandle,
    ) -> SemanticsNode {
        let m = self.meta.get(&node);
        let mut s = SemanticsNode::new(
            handle,
            node.index(),
            m.map(|m| m.role).unwrap_or(Role::Generic),
        );
        if let Some(m) = m {
            s.id = m.id.clone();
            s.label = m.label.clone();
            s.value = m.value.clone();
            s.classes = m.classes.clone();
            s.actions = m.actions.clone();
            #[cfg(feature = "dev-observability")]
            {
                s.type_name = m.role.type_name();
            }
            s.elide = m.elide;
            s.overlay = m.overlay;
            s.scroll = m.scroll().copied();
            s.set_size = m.rare.as_ref().and_then(|r| r.set_size);
            s.position_in_set = m.rare.as_ref().and_then(|r| r.position_in_set);
            s.states = m.states.clone();
            let flags = self.tree.flags(node);
            if flags.contains(NodeFlags::FOCUSED) {
                s.states.push(SemState::Focused);
            }
            if flags.contains(NodeFlags::HOVERED) {
                s.states.push(SemState::Hovered);
            }
            if flags.contains(NodeFlags::DISABLED) {
                s.states.push(SemState::Disabled);
            }
        }
        s.bounds = self.tree.bounds(node);
        #[cfg(feature = "dev-observability")]
        {
            s.deps = m.and_then(|m| (!m.deps.is_empty()).then(|| m.deps.union()));
            s.ink = self.node_ink.get(&node).copied();
            s.text_metrics =
                self.node_text_metrics
                    .get(&node)
                    .map(|m| lumen_core::semantics::TextMetrics {
                        line_count: m.line_count as u32,
                        box_height: m.box_height,
                        ascent: m.ascent,
                        descent: m.descent,
                        line_height: m.line_height,
                        content_height: m.content_height,
                    });
        }
        let mut child = self.tree.first_child(node);
        // Ordinal counts only the children that survive into the semantic tree,
        // so a hidden sibling appearing or disappearing does not renumber — and
        // therefore does not re-identify — the ones around it.
        let mut ordinal = 0u32;
        while child.is_some() {
            // B.3 visibility:hidden — hidden subtrees leave the semantic tree
            // too (what the agent sees matches what the user sees).
            if self.tree.flags(child).contains(NodeFlags::VISIBLE) {
                let cm = self.meta.get(&child);
                let crole = cm.map(|m| m.role).unwrap_or(Role::Generic);
                let ch = match cm.and_then(|m| m.id.as_ref()) {
                    Some(id) => handle.child(id.as_str()),
                    None => handle.child(&(crole.as_str(), ordinal)),
                };
                s.children.push(self.build_semantics_at(child, ch));
                ordinal += 1;
            }
            child = self.tree.next_sibling(child);
        }
        s
    }
}

/// Extract a human-readable message from a caught panic payload.
fn panic_msg(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_string()
    }
}

/// A hover-state version of a control colour: lighten a dark fill, darken a
/// light one (perceptually, in Oklab). Subtle but visible.
/// Apply a resolved style's layout + typography fields onto the element
/// before it is lowered (A.2 pre-layout merge; shared by the sheet path and
/// the sheet-less inline path). Only the fields the style actually set win
/// over the element's `LayoutStyle` (element < .lss < inline, 04 §2).
fn apply_css_to_element(el: &mut Element, css: &lumen_style::Style) {
    if let Some(d) = css.display {
        el.style.display = d;
    }
    if let Some(f) = css.flex_direction {
        el.style.flex_direction = f;
    }
    if let Some(w) = css.width {
        el.style.width = w;
    }
    if let Some(h) = css.height {
        el.style.height = h;
    }
    if let Some(g) = css.gap {
        el.style.row_gap = g;
        el.style.column_gap = g;
    }
    // PROP1: per-axis gaps come after the shorthand so they override it,
    // matching CSS source-order intuition (same rule the padding longhands use
    // below).
    if let Some(g) = css.row_gap {
        el.style.row_gap = g;
    }
    if let Some(g) = css.column_gap {
        el.style.column_gap = g;
    }
    if let Some(a) = css.justify_content {
        el.style.justify_content = Some(a);
    }
    if let Some(a) = css.align_items {
        el.style.align_items = Some(a);
    }
    if let Some(a) = css.align_self {
        el.style.align_self = Some(a);
    }
    if let Some(w) = css.flex_wrap {
        el.style.flex_wrap = w;
    }
    if let Some(n) = css.flex_grow {
        el.style.flex_grow = n;
    }
    if let Some(n) = css.flex_shrink {
        el.style.flex_shrink = n;
    }
    // PROP1: these `LayoutStyle` fields existed and taffy has always
    // implemented them; only this bridge and `Style::apply` were missing, so
    // the properties parsed and were silently discarded.
    if let Some(d) = css.flex_basis {
        el.style.flex_basis = d;
    }
    if let Some(a) = css.align_content {
        el.style.align_content = Some(a);
    }
    if let Some(r) = css.aspect_ratio {
        el.style.aspect_ratio = Some(r);
    }
    if let Some(p) = css.position {
        el.style.position = p;
    }
    if let Some(i) = css.inset {
        el.style.inset = i;
    }
    if let Some(t) = &css.grid_template_columns {
        el.style.grid_template_columns = t.clone();
    }
    if let Some(t) = &css.grid_template_rows {
        el.style.grid_template_rows = t.clone();
    }
    if let Some(p) = css.grid_column {
        el.style.grid_column = p;
    }
    if let Some(p) = css.grid_row {
        el.style.grid_row = p;
    }
    if let Some(d) = css.min_width {
        el.style.min_width = d;
    }
    if let Some(d) = css.min_height {
        el.style.min_height = d;
    }
    if let Some(d) = css.max_width {
        el.style.max_width = d;
    }
    if let Some(d) = css.max_height {
        el.style.max_height = d;
    }
    if let Some(p) = css.padding {
        el.style.padding = p;
    }
    if let Some(m) = css.margin {
        el.style.margin = m;
    }
    // B.3 longhands: per-side values override component-wise (after the
    // whole-side shorthand, matching CSS source-order intuition).
    let [pt, pr, pb, pl] = css.padding_sides;
    if let Some(v) = pt {
        el.style.padding.top = Dim::px(v);
    }
    if let Some(v) = pr {
        el.style.padding.right = Dim::px(v);
    }
    if let Some(v) = pb {
        el.style.padding.bottom = Dim::px(v);
    }
    if let Some(v) = pl {
        el.style.padding.left = Dim::px(v);
    }
    let [mt, mr, mb, ml] = css.margin_sides;
    if let Some(v) = mt {
        el.style.margin.top = Dim::px(v);
    }
    if let Some(v) = mr {
        el.style.margin.right = Dim::px(v);
    }
    if let Some(v) = mb {
        el.style.margin.bottom = Dim::px(v);
    }
    if let Some(v) = ml {
        el.style.margin.left = Dim::px(v);
    }
    // PROP1: `inset` longhands, after the shorthand, same rule as padding/margin.
    let [it, ir, ib, il] = css.inset_sides;
    if let Some(v) = it {
        el.style.inset.top = Dim::px(v);
    }
    if let Some(v) = ir {
        el.style.inset.right = Dim::px(v);
    }
    if let Some(v) = ib {
        el.style.inset.bottom = Dim::px(v);
    }
    if let Some(v) = il {
        el.style.inset.left = Dim::px(v);
    }
    // B.4: typography reaches the text stack — the measured and the painted
    // TextStyle are the same object (content moves into NodeMeta), so one
    // override covers both passes.
    if css.font_size.is_some()
        || css.font_weight.is_some()
        || css.line_height.is_some()
        || css.letter_spacing.is_some()
        || css.font_family.is_some()
        || css.text_align.is_some()
        || css.font_italic.is_some()
        || css.font_features.is_some()
        || css.font_variations.is_some()
    {
        if let NodeContent::Text(_, ts) = &mut el.content {
            if let Some(fs) = css.font_size {
                ts.font_size = fs;
            }
            if let Some(w) = css.font_weight {
                ts.weight = w as f32;
            }
            if let Some(lh) = css.line_height {
                ts.line_height = Some(lh);
            }
            // PROP1: both fields already existed on `TextStyle`; only this
            // bridge was missing, so the declarations parsed and vanished.
            if let Some(ls) = css.letter_spacing {
                ts.letter_spacing = ls;
            }
            if let Some(fam) = &css.font_family {
                ts.family = Some(fam.clone());
            }
            if let Some(a) = css.text_align {
                ts.align = a;
            }
            if let Some(i) = css.font_italic {
                ts.italic = i;
            }
            if let Some(f) = &css.font_features {
                ts.features = (!f.is_empty()).then(|| f.clone());
            }
            if let Some(v) = &css.font_variations {
                ts.variations = (!v.is_empty()).then(|| v.clone());
            }
        }
    }
}

/// B.6b: merge a typed inline style (`Origin::Inline`) over the resolved
/// sheet style, field-wise. A sheet declaration marked `!important` keeps the
/// field (04 §2: `!important` beats inline). Representable values also land
/// in the computed map with `source: "inline"` for `ui.getStyles`; compound
/// fields (gradients, shadows, per-side arrays, backdrop) apply to paint and
/// layout but are not serialized (documented in 04 §8).
fn merge_inline_style(
    css: &mut lumen_style::Style,
    resolved: &mut HashMap<String, lumen_style::Computed>,
    inline: &lumen_style::Style,
) {
    use lumen_style::{Computed, Origin, Unit, Value};
    fn imp(r: &HashMap<String, lumen_style::Computed>, p: &str) -> bool {
        r.get(p).is_some_and(|c| c.important)
    }
    fn put(r: &mut HashMap<String, lumen_style::Computed>, p: &str, v: Value) {
        r.insert(
            p.to_string(),
            Computed {
                value: v,
                important: false,
                origin: Origin::Inline,
                span: None,
            },
        );
    }
    let dim_value = |d: &lumen_layout::Dim| match d {
        lumen_layout::Dim::Px(v) => Some(Value::Number(*v as f64, Unit::Px)),
        lumen_layout::Dim::Percent(v) => Some(Value::Number((*v * 100.0) as f64, Unit::Percent)),
        _ => None,
    };
    macro_rules! field {
        // css assignment only (compound / enum values)
        ($f:ident, $p:literal) => {
            if inline.$f.is_some() && !imp(resolved, $p) {
                css.$f = inline.$f.clone();
            }
        };
        // + computed-map entry via $to(value) -> Option<Value>
        ($f:ident, $p:literal, $to:expr) => {
            if let Some(v) = &inline.$f {
                if !imp(resolved, $p) {
                    css.$f = inline.$f.clone();
                    #[allow(clippy::redundant_closure_call)]
                    if let Some(val) = ($to)(v) {
                        put(resolved, $p, val);
                    }
                }
            }
        };
    }
    let color = |c: &lumen_core::Color| Some(Value::Color(*c));
    let px = |v: &f32| Some(Value::Number(*v as f64, Unit::Px));
    let num = |v: &f32| Some(Value::Number(*v as f64, Unit::None));
    field!(background, "background", color);
    field!(color, "color", color);
    field!(border_color, "border-color", color);
    field!(width, "width", dim_value);
    field!(height, "height", dim_value);
    field!(gap, "gap", dim_value);
    field!(border_radius, "border-radius", px);
    field!(border_width, "border-width", px);
    field!(font_size, "font-size", px);
    field!(opacity, "opacity", num);
    field!(line_height, "line-height", num);
    field!(font_weight, "font-weight", |v: &u16| Some(Value::Number(
        *v as f64,
        Unit::None
    )));
    field!(visibility, "visibility", |v: &bool| Some(Value::Keyword(
        if *v { "visible" } else { "hidden" }.to_string()
    )));
    // css-only fields: paint/layout honor them; not serialized (04 §8 note).
    field!(display, "display");
    field!(flex_direction, "flex-direction");
    field!(padding, "padding");
    field!(margin, "margin");
    field!(background_gradient, "background");
    field!(shadow, "shadow");
    field!(clip, "clip");
    field!(blend_mode, "blend-mode");
    field!(border_radius_corners, "border-radius");
    for i in 0..4 {
        if inline.padding_sides[i].is_some() {
            css.padding_sides[i] = inline.padding_sides[i];
        }
        if inline.margin_sides[i].is_some() {
            css.margin_sides[i] = inline.margin_sides[i];
        }
        if inline.border_sides[i].is_some() {
            css.border_sides[i] = inline.border_sides[i];
        }
    }
    if inline.backdrop_blur.is_some() && !imp(resolved, "backdrop-filter") {
        css.backdrop_blur = inline.backdrop_blur;
    }
    if inline.backdrop_saturate.is_some() && !imp(resolved, "backdrop-filter") {
        css.backdrop_saturate = inline.backdrop_saturate;
    }
}

/// A.3.5 bisect hatch: `LUMEN_FULL_REBUILD=1` disables copy-forward and the
/// restyle-only visual path, so a live run behaves like the coherence
/// oracle's rebuild-fresh — isolate a suspected incremental bug in seconds.
fn full_rebuild_forced() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| std::env::var("LUMEN_FULL_REBUILD").is_ok_and(|v| v == "1"))
}

/// Whether two computed styles differ in any property that feeds layout or
/// text measurement (A.5) — if so, a visual-state restyle must escalate to a
/// full rebuild so the new geometry is real.
fn layout_affecting_differ(old: Option<&Styled>, new: &lumen_style::Style) -> bool {
    let d = lumen_style::Style::new();
    let old = old.map(|o| &**o).unwrap_or(&d);
    old.display != new.display
        || old.flex_direction != new.flex_direction
        || old.width != new.width
        || old.height != new.height
        || old.gap != new.gap
        || old.padding != new.padding
        || old.margin != new.margin
        || old.padding_sides != new.padding_sides
        || old.margin_sides != new.margin_sides
        || old.font_size != new.font_size
        || old.font_weight != new.font_weight
        || old.line_height != new.line_height
        || old.visibility != new.visibility
}

/// Map a box-relative `.lss` gradient onto the renderer brush for `bounds`
/// (B.3). Linear: CSS angle (0 = to top, 90 = to right), line through the
/// center sized by the box's projection onto the axis. Radial: centered,
/// farthest-corner radius.
fn gradient_brush(g: &lumen_style::StyleGradient, bounds: Rect) -> Brush {
    let stops: Vec<lumen_render::GradientStop> = g
        .stops
        .iter()
        .map(|(o, c)| lumen_render::GradientStop {
            offset: *o,
            color: *c,
        })
        .collect();
    let c = bounds.center();
    match g.angle_deg {
        Some(deg) => {
            let a = (deg as f64).to_radians();
            let (dx, dy) = (a.sin(), -a.cos());
            let half = (bounds.width() * dx.abs() + bounds.height() * dy.abs()) / 2.0;
            Brush::LinearGradient {
                start: kurbo::Point::new(c.x - dx * half, c.y - dy * half),
                end: kurbo::Point::new(c.x + dx * half, c.y + dy * half),
                stops,
                spread: lumen_render::SpreadMode::Pad,
            }
        }
        None => Brush::RadialGradient {
            center: c,
            radius: ((bounds.width() / 2.0).powi(2) + (bounds.height() / 2.0).powi(2)).sqrt(),
            stops,
            spread: lumen_render::SpreadMode::Pad,
        },
    }
}

/// Hover feedback for gradient fills: tint every stop the way
/// [`hover_tint`] treats a solid.
fn hover_tint_brush(brush: &mut Brush) {
    match brush {
        Brush::Solid(c) => *c = hover_tint(*c),
        Brush::LinearGradient { stops, .. } | Brush::RadialGradient { stops, .. } => {
            for stop in stops {
                stop.color = hover_tint(stop.color);
            }
        }
        _ => {}
    }
}

/// How far a finger may travel before a press stops counting as a tap.
///
/// A touch that moves further is scrolling: the row under it will still be the
/// same row when the finger lifts (the content moved with it), so "released on
/// the node it pressed" cannot tell a tap from a drag on its own — this can.
///
/// 10 px sits between Android's ~8 dp `ViewConfiguration` slop and Flutter's
/// 18 px `kTouchSlop`. Lower is more sensitive to a jittery finger cancelling a
/// deliberate tap; higher lets the first few pixels of a scroll still activate
/// what it started on.
const TOUCH_SLOP_PX: f64 = 10.0;

/// Whether a node with a wheel handler should consume this event.
///
/// Deliberately NOT directional ("can it move *this* way"). That is the right
/// long-term rule, but `PullToRefresh` declares a fictional `max_y: 1e6` and
/// relies on receiving `dy < 0` while already at the top in order to fire —
/// a directional test would see no room upward, chain the event away, and break
/// it silently. Widening this is gated on making that `ScrollInfo` honest.
///
/// A node with no `ScrollInfo` is not a scroll container at all (a stepper, a
/// zoomable grid); it consumes, exactly as before.
///
/// One consequence worth stating: a single `WheelHandler` call carries both
/// `dx` and `dy`, so consumption cannot be split per axis. The decision is
/// all-or-nothing per event.
fn wheel_can_take(scroll: Option<lumen_core::semantics::ScrollInfo>) -> bool {
    match scroll {
        None => true,
        Some(s) => s.max_x > 0.5 || s.max_y > 0.5,
    }
}

fn hover_tint(c: Color) -> Color {
    let lum = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    let target = if lum < 0.5 {
        Color::WHITE
    } else {
        Color::BLACK
    };
    c.lerp_oklab(target, 0.12)
}

/// Helper: the center point of a rect (for synthesized clicks).
pub fn center(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

/// Re-export so callers can build the default window background.
pub const WINDOW_BG: Color = Color::WHITE;

/// A default style alias used by examples.
pub type Style = LayoutStyle;
