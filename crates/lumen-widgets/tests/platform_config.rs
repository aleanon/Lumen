//! MOD1: the platform bundle must actually select the runtime's internals.
//!
//! MOD2 and MOD3 proved their traits are *implementable* from outside. This
//! proves the bundle is *wired*: an `App` parameterised with a different
//! `PlatformConfig` must run on that config's engines, not on the defaults.
//!
//! Without this test the bundle would be a trait the runtime declares and never
//! consults — which is the same "interface with one caller and one callee"
//! failure the seam tests exist to rule out, moved up a level.

use kurbo::Size;
use lumen_text::{CachedRun, TextAlign, TextBlockApi, TextEngineApi, TextMetrics, TextStyle};
use lumen_widgets::app::PlatformConfig;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A text engine with deliberately WRONG metrics: every glyph is 10px wide and
/// every line 30px tall, regardless of font size. If the runtime is really
/// using it, laid-out text boxes will match those numbers rather than the
/// bundled face's — which is how this test tells the two apart.
const ADV: f32 = 10.0;
const LINE: f32 = 30.0;

#[derive(Default)]
struct FixedEngine {
    last: Option<FixedBlock>,
    last_run: Option<CachedRun>,
}

#[derive(Clone, Default)]
struct FixedBlock {
    chars: usize,
}

impl TextBlockApi for FixedBlock {
    fn width(&self) -> f32 {
        self.chars as f32 * ADV
    }
    fn height(&self) -> f32 {
        LINE
    }
    fn size(&self) -> Size {
        Size::new(self.width() as f64, self.height() as f64)
    }
    fn metrics(&self) -> TextMetrics {
        TextMetrics {
            line_count: 1,
            box_height: LINE,
            ascent: LINE * 0.8,
            descent: LINE * 0.2,
            line_height: LINE,
            content_height: LINE,
        }
    }
    fn missing_glyphs(&self) -> usize {
        0
    }
    fn caret_pos(&self, byte: usize) -> (f32, f32, f32) {
        (byte as f32 * ADV, 0.0, LINE)
    }
    fn hit_to_byte(&self, x: f32, _y: f32) -> usize {
        ((x / ADV).round().max(0.0) as usize).min(self.chars)
    }
    fn selection_rects(&self, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)> {
        vec![(a as f32 * ADV, 0.0, (b as f32 - a as f32) * ADV, LINE)]
    }
    fn render(
        &self,
        width: u32,
        height: u32,
        background: lumen_core::Color,
    ) -> lumen_render::RgbaImage {
        let [r, g, b, a] = background.to_srgb8();
        let mut px = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            px.extend_from_slice(&[r, g, b, a]);
        }
        lumen_render::RgbaImage::from_raw(width, height, px)
    }
}

impl TextEngineApi for FixedEngine {
    type Block = FixedBlock;

    fn begin_frame(&mut self) {}
    fn register_font(&mut self, _bytes: Vec<u8>) -> Option<String> {
        None
    }
    fn shaped(
        &mut self,
        text: &str,
        _base: &TextStyle,
        _max_width: Option<f32>,
        _align: TextAlign,
    ) -> &FixedBlock {
        self.last = Some(FixedBlock {
            chars: text.chars().count(),
        });
        self.last.as_ref().unwrap()
    }
    fn shaped_run(
        &mut self,
        text: &str,
        _base: &TextStyle,
        _max_width: Option<f32>,
        _align: TextAlign,
        _scale: f32,
    ) -> &CachedRun {
        let block = FixedBlock {
            chars: text.chars().count(),
        };
        // No glyphs: this engine measures but does not rasterize. The runtime
        // must survive that — a text stack without a rasterizer is a legitimate
        // substitution (a measuring-only backend for a server-side layout pass).
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
        _base: TextStyle,
        _ranges: &[(std::ops::Range<usize>, TextStyle)],
        _max_width: Option<f32>,
        _align: TextAlign,
    ) -> FixedBlock {
        FixedBlock {
            chars: text.chars().count(),
        }
    }
}

/// A bundle that keeps the default layout engine and swaps only text — which is
/// the point of a bundle: seams are selected independently.
struct FixedTextPlatform;

impl PlatformConfig for FixedTextPlatform {
    type Layout = lumen_layout::LayoutTree;
    type Text = FixedEngine;
}

fn app() -> impl Fn(&mut BuildCx) -> Element {
    |_cx: &mut BuildCx| widgets::column(vec![widgets::text("abcde").id("lbl")]).id("root")
}

#[test]
fn a_custom_platform_actually_drives_layout() {
    let mut h =
        App::<_, _, FixedTextPlatform>::with_platform(app()).run_headless(Size::new(400.0, 200.0));
    h.pump();
    let b = h.node_bounds_by_id("lbl").expect("label laid out");

    // 5 chars x 10px, and the engine's fixed 30px line height. Neither number
    // is producible by the bundled parley face at the default 16px size, so
    // matching them proves the runtime consulted the bundle.
    assert!(
        (b.width() - 50.0).abs() < 0.5,
        "expected the custom engine's 5x10px advance, got {}",
        b.width()
    );
    assert!(
        (b.height() - 30.0).abs() < 0.5,
        "expected the custom engine's 30px line, got {}",
        b.height()
    );
}

/// The default bundle must be unchanged — the parameter is defaulted, so every
/// existing `App::new(...)` keeps taffy + parley without naming anything.
#[test]
fn the_default_platform_is_unchanged() {
    let mut h = App::new(app()).run_headless(Size::new(400.0, 200.0));
    h.pump();
    let b = h.node_bounds_by_id("lbl").expect("label laid out");
    assert!(
        (b.width() - 50.0).abs() > 0.5 || (b.height() - 30.0).abs() > 0.5,
        "the default build must NOT be using the test engine's metrics"
    );
    assert!(b.width() > 0.0 && b.height() > 0.0);
}
