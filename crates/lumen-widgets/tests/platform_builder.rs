//! MOD7 S0: the typestate builder must carry the `PlatformConfig` through.
//!
//! `with_renderer` and `with_executor` used to be typed `-> App<R2, E>` and
//! `-> App<R, E2>`. The third parameter was simply absent, so it fell back to
//! `DefaultPlatform` — an app built with `with_platform(..)` reverted to the
//! shipped taffy + parley bundle the moment either was called, and ran on
//! engines the author had explicitly replaced.
//!
//! Nothing caught it because nothing was wrong at the *call site*: the code
//! compiles, and only annotating the result surfaces
//! `expected App<_, _, MyPlatform>, found App<_, _, DefaultPlatform>`. So the
//! guard cannot be "does it compile" — it has to observe the engine actually
//! driving layout, which is what this does.

use kurbo::Size;
use lumen_text::{CachedRun, TextAlign, TextBlockApi, TextEngineApi, TextMetrics, TextStyle};
use lumen_widgets::app::PlatformConfig;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Deliberately wrong metrics: 10 px per char, 30 px lines, at any font size.
/// The bundled face cannot produce these numbers, which is how the assertions
/// below tell the two engines apart.
const ADV: f32 = 10.0;
const LINE: f32 = 30.0;

#[derive(Default)]
struct MarkerEngine {
    last: Option<MarkerBlock>,
    last_run: Option<CachedRun>,
}

#[derive(Clone, Default)]
struct MarkerBlock {
    chars: usize,
}

impl TextBlockApi for MarkerBlock {
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
    fn caret_pos(&self, byte: usize) -> (f32, f32, f32) {
        (byte as f32 * ADV, 0.0, LINE)
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

impl TextEngineApi for MarkerEngine {
    type Block = MarkerBlock;
    fn register_font(&mut self, _bytes: Vec<u8>) -> Option<String> {
        None
    }
    fn begin_frame(&mut self) {}
    fn shaped(
        &mut self,
        text: &str,
        _s: &TextStyle,
        _w: Option<f32>,
        _a: TextAlign,
    ) -> &Self::Block {
        self.last = Some(MarkerBlock {
            chars: text.chars().count(),
        });
        self.last.as_ref().unwrap()
    }
    fn shaped_run(
        &mut self,
        text: &str,
        _b: &TextStyle,
        _w: Option<f32>,
        _a: TextAlign,
        _s: f32,
    ) -> &CachedRun {
        let block = MarkerBlock {
            chars: text.chars().count(),
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
        text: &str,
        _b: TextStyle,
        _r: &[(std::ops::Range<usize>, TextStyle)],
        _w: Option<f32>,
        _a: TextAlign,
    ) -> MarkerBlock {
        MarkerBlock {
            chars: text.chars().count(),
        }
    }
}

struct MarkerPlatform;
impl PlatformConfig for MarkerPlatform {
    type Layout = lumen_layout::LayoutTree;
    type Text = MarkerEngine;
}

fn app() -> impl Fn(&mut BuildCx) -> Element {
    |_cx: &mut BuildCx| widgets::column(vec![widgets::text("abcde").id("lbl")]).id("root")
}

/// 5 chars x 10 px wide, 30 px tall — the marker engine's numbers, which the
/// bundled face cannot produce at any size.
fn assert_marker_engine_drove_layout<R, E>(h: &mut lumen_widgets::Headless<R, E, MarkerPlatform>)
where
    R: lumen_render::Renderer,
    E: lumen_core::tasks::Spawner,
{
    h.pump();
    let b = h.node_bounds_by_id("lbl").expect("label laid out");
    assert!(
        (b.width() - 50.0).abs() < 0.5 && (b.height() - 30.0).abs() < 0.5,
        "the platform was lost: expected the marker engine's 50x30px, got {}x{}",
        b.width(),
        b.height()
    );
}

#[test]
fn with_renderer_keeps_the_platform() {
    let mut h = App::<_, _, MarkerPlatform>::with_platform(app())
        .with_renderer(lumen_render::TinySkia)
        .run_headless(Size::new(400.0, 200.0));
    assert_marker_engine_drove_layout(&mut h);
}

#[test]
fn with_executor_keeps_the_platform() {
    let mut h = App::<_, _, MarkerPlatform>::with_platform(app())
        .with_executor(lumen_core::tasks::InlineSpawner)
        .run_headless(Size::new(400.0, 200.0));
    assert_marker_engine_drove_layout(&mut h);
}

/// Both at once, in the order the shell uses them — this is the combination
/// that was unreachable before S0, and the shell needs it for MOD7 S1.
#[test]
fn a_full_builder_chain_keeps_the_platform() {
    let mut h = App::<_, _, MarkerPlatform>::with_platform(app())
        .stylesheet("#root { padding: 0px; }")
        .with_renderer(lumen_render::TinySkia)
        .with_executor(lumen_core::tasks::InlineSpawner)
        .run_headless(Size::new(400.0, 200.0));
    assert_marker_engine_drove_layout(&mut h);
}
