//! A Lumen app whose text engine is a fixed-metrics stub, selected through
//! `PlatformConfig`. Measures nothing real — the point is what the LINKER does
//! with the parley/swash/ICU stack once nothing instantiates it.
#![allow(dead_code)] // see the control note below
use lumen_core::geometry::Size;
use lumen_text::{CachedRun, TextAlign, TextBlockApi, TextEngineApi, TextMetrics, TextStyle};
use lumen_widgets::app::PlatformConfig;
use lumen_widgets::{widgets, App, BuildCx, Element};

// The stub engine below is deliberately still COMPILED into this variant and
// deliberately never named by `PlatformConfig`. That is the control: both
// binaries contain the same source, so the ~6 MB difference between them
// cannot be "the other one has less code in it" — it is the linker dropping
// parley/swash/skrifa/harfrust, the ICU tables and the embedded fonts once
// nothing instantiates `lumen_text::TextEngine`.
const ADV: f32 = 8.0;
const LINE: f32 = 18.0;

#[derive(Default)]
struct StubEngine {
    last_run: Option<CachedRun>,
}
#[derive(Clone, Default)]
struct StubBlock {
    chars: usize,
}

impl TextBlockApi for StubBlock {
    fn width(&self) -> f32 { self.chars as f32 * ADV }
    fn height(&self) -> f32 { LINE }
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
    fn missing_glyphs(&self) -> usize { 0 }
    fn caret_pos(&self, byte: usize) -> (f32, f32, f32) { (byte as f32 * ADV, 0.0, LINE) }
    fn hit_to_byte(&self, x: f32, _y: f32) -> usize { (x / ADV).max(0.0) as usize }
    fn selection_rects(&self, _a: usize, _b: usize) -> Vec<(f32, f32, f32, f32)> { Vec::new() }
    fn render(&self, width: u32, height: u32, _bg: lumen_core::Color) -> lumen_render::RgbaImage {
        lumen_render::RgbaImage::from_raw(width, height, vec![0; (width * height * 4) as usize])
    }
}

impl TextEngineApi for StubEngine {
    type Block = StubBlock;
    fn register_font(&mut self, _bytes: Vec<u8>) -> Option<String> { None }
    fn begin_frame(&mut self) {}
    fn shaped(&mut self, text: &str, _s: &TextStyle, _w: Option<f32>, _a: TextAlign) -> &Self::Block {
        Box::leak(Box::new(StubBlock { chars: text.chars().count() }))
    }
    fn shaped_run(&mut self, text: &str, _b: &TextStyle, _w: Option<f32>, _a: TextAlign, _s: f32) -> &CachedRun {
        let block = StubBlock { chars: text.chars().count() };
        self.last_run = Some(CachedRun {
            run: lumen_render::GlyphRun::default(),
            images: Vec::new(),
            ink: [0.0, 0.0, block.width(), block.height()],
            metrics: block.metrics(),
        });
        self.last_run.as_ref().unwrap()
    }
    fn layout(&mut self, text: &str, _b: TextStyle, _r: &[(std::ops::Range<usize>, TextStyle)],
              _w: Option<f32>, _a: TextAlign) -> StubBlock {
        StubBlock { chars: text.chars().count() }
    }
}

struct StubPlatform;
impl PlatformConfig for StubPlatform {
    type Layout = lumen_layout::LayoutTree;
    type Text = lumen_text::TextEngine; // <-- the ONLY difference
}

fn view(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i64);
    widgets::column(vec![
        widgets::text("ABCDEFGH").id("lbl"),
        widgets::button("Increment", move |rt| count.set(rt, count.get(rt) + 1)),
    ])
}

fn main() {
    // Headless deliberately: `lumen_shell::run` takes a fully-defaulted `App`,
    // so a custom `PlatformConfig` has no windowed entry point today. Depending
    // on lumen-shell at all would re-instantiate the default TextEngine and
    // pull parley straight back in, which would defeat the measurement.
    let mut h = App::<_, _, StubPlatform>::with_platform(view)
        .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let b = h.node_bounds_by_id("lbl").expect("label laid out");
    // Self-verifying: 8 chars x 8 px = 64 px wide and 18 px tall under the
    // stub; the parley stack produces neither number. If this prints the
    // stub's metrics, the binary genuinely is not running parley — which is
    // what makes the size figure beside it mean something.
    println!("engine = lumen_text::TextEngine");
    println!("  \"ABCDEFGH\" laid out {:.1} x {:.1} px   (real parley/swash stack)", b.width(), b.height());
}
