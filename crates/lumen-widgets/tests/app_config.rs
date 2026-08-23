//! MOD7 S2: one config names all four swap axes.
//!
//! `App<R, E, P>` keeps its three parameters — varying a single axis
//! (`with_renderer`, the shell's own `Box<dyn Renderer>`) is a real use, and a
//! fused parameter would force a whole new config to change one thing. This is
//! the ergonomic layer on top: a consumer writes `ConfiguredApp<MyConfig>` and
//! names one type.
//!
//! What has to hold is that the alias and the constructor actually deliver the
//! config's choices, rather than being a nicer spelling of the defaults.

use kurbo::Size;
use lumen_text::{CachedRun, TextAlign, TextBlockApi, TextEngineApi, TextMetrics, TextStyle};
use lumen_widgets::app::{AppConfig, ConfiguredApp};
use lumen_widgets::{widgets, BuildCx, Element};

const ADV: f32 = 7.0;
const LINE: f32 = 25.0;

#[derive(Default)]
struct TinyEngine {
    last: Option<TinyBlock>,
    last_run: Option<CachedRun>,
}

#[derive(Clone, Default)]
struct TinyBlock {
    chars: usize,
}

impl TextBlockApi for TinyBlock {
    fn width(&self) -> f32 {
        self.chars as f32 * ADV
    }
    fn height(&self) -> f32 {
        LINE
    }
    fn size(&self) -> lumen_core::geometry::Size {
        lumen_core::geometry::Size::new(self.width() as f64, LINE as f64)
    }
    fn metrics(&self) -> TextMetrics {
        TextMetrics {
            ascent: LINE * 0.8,
            descent: LINE * 0.2,
            line_height: LINE,
            line_count: 1,
            content_height: LINE,
            box_height: LINE,
        }
    }
    fn missing_glyphs(&self) -> usize {
        0
    }
    fn caret_pos(&self, b: usize) -> (f32, f32, f32) {
        (b as f32 * ADV, 0.0, LINE)
    }
    fn hit_to_byte(&self, x: f32, _y: f32) -> usize {
        (x / ADV).max(0.0) as usize
    }
    fn selection_rects(&self, _a: usize, _b: usize) -> Vec<(f32, f32, f32, f32)> {
        Vec::new()
    }
    fn render(&self, w: u32, h: u32, _bg: lumen_core::Color) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(w, h, vec![0; (w * h * 4) as usize])
    }
}

impl TextEngineApi for TinyEngine {
    type Block = TinyBlock;
    fn register_font(&mut self, _b: Vec<u8>) -> Option<String> {
        None
    }
    fn begin_frame(&mut self) {}
    fn shaped(&mut self, t: &str, _s: &TextStyle, _w: Option<f32>, _a: TextAlign) -> &TinyBlock {
        self.last = Some(TinyBlock {
            chars: t.chars().count(),
        });
        self.last.as_ref().unwrap()
    }
    fn shaped_run(
        &mut self,
        t: &str,
        _b: &TextStyle,
        _w: Option<f32>,
        _a: TextAlign,
        _s: f32,
    ) -> &CachedRun {
        let block = TinyBlock {
            chars: t.chars().count(),
        };
        self.last_run = Some(CachedRun {
            run: lumen_render::GlyphRun::default(),
            images: Vec::new(),
            ink: [0.0, 0.0, block.width(), block.height()],
            metrics: block.metrics(),
        });
        self.last_run.as_ref().unwrap()
    }
    fn layout(
        &mut self,
        t: &str,
        _b: TextStyle,
        _r: &[(std::ops::Range<usize>, TextStyle)],
        _w: Option<f32>,
        _a: TextAlign,
    ) -> TinyBlock {
        TinyBlock {
            chars: t.chars().count(),
        }
    }
}

/// A config that differs from the defaults on every axis it can: a boxed
/// renderer (which is also the case a `Default` bound could not express — the
/// reason `AppConfig` uses factory functions), a thread-pool executor, and the
/// tiny text engine above.
struct TinyConfig;

impl AppConfig for TinyConfig {
    type Renderer = Box<dyn lumen_widgets::Renderer>;
    type Executor = lumen_core::tasks::ThreadPoolSpawner;
    type Layout = lumen_layout::LayoutTree;
    type Text = TinyEngine;
    fn renderer() -> Self::Renderer {
        Box::new(lumen_render::TinySkia)
    }
    fn executor() -> Self::Executor {
        lumen_core::tasks::ThreadPoolSpawner::new(1)
    }
}

fn view(_cx: &mut BuildCx) -> Element {
    widgets::column(vec![widgets::text("abcdefg").id("lbl")])
}

/// The whole point: one named type produces a runtime on that config's engines.
/// 7 chars x 7 px and a 25 px line are numbers the bundled parley face cannot
/// produce at any size, so matching them proves the config was consulted rather
/// than politely ignored.
#[test]
fn one_config_selects_the_text_engine() {
    let mut h =
        ConfiguredApp::<TinyConfig>::with_config(view).run_headless(Size::new(300.0, 150.0));
    h.pump();
    let b = h.node_bounds_by_id("lbl").expect("laid out");
    assert!(
        (b.width() - 49.0).abs() < 0.5 && (b.height() - 25.0).abs() < 0.5,
        "expected the config's 49x25px, got {}x{}",
        b.width(),
        b.height()
    );
}

/// A boxed renderer is the case that made `AppConfig` use factory functions
/// instead of a `Default` bound — `Box<dyn Renderer>` cannot implement
/// `Default`. If this compiles and runs, the factory shape is doing its job.
#[test]
fn a_config_can_name_a_boxed_renderer() {
    let mut h =
        ConfiguredApp::<TinyConfig>::with_config(view).run_headless(Size::new(300.0, 150.0));
    let stats = h.pump();
    assert!(
        stats.node_count > 0,
        "the boxed-renderer config built nothing"
    );
}

/// The builder still works on a configured app — S0's guarantee has to survive
/// S2, or swapping one axis of a config means rebuilding the whole config.
#[test]
fn a_configured_app_still_takes_builder_calls() {
    let mut h = ConfiguredApp::<TinyConfig>::with_config(view)
        .stylesheet("#lbl { padding: 0px; }")
        .run_headless(Size::new(300.0, 150.0));
    h.pump();
    let b = h.node_bounds_by_id("lbl").expect("laid out");
    assert!(
        (b.width() - 49.0).abs() < 0.5,
        "the config's text engine was lost through a builder call: {}",
        b.width()
    );
}
