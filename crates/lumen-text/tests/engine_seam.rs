//! MOD3: the text seam must be implementable by someone who is not Lumen.
//!
//! A trait the framework declares and only the framework implements is not an
//! extension point — it is an interface with one caller and one callee, which
//! proves nothing about substitutability. So this test implements a complete
//! alternative text engine from outside the crate, using only public API.
//!
//! It is intentionally trivial (a fixed-advance monospace grid, no shaping,
//! no fallback), because the property under test is "can this be written at
//! all", not "is it a good engine". If it compiles and satisfies the same
//! contract the runtime relies on, the seam is real.

use kurbo::Size;
use lumen_core::Color;
use lumen_render::RgbaImage;
use lumen_text::{TextAlign, TextBlockApi, TextEngineApi, TextMetrics, TextStyle};

/// Every glyph is this many px wide, and every line this tall, per 1px of font
/// size. Nothing about the numbers matters — only that they are self-consistent.
const ADV: f32 = 0.6;
const LINE: f32 = 1.2;

#[derive(Default)]
struct GridEngine {
    /// Proves the frame hook is reachable and can drive an eviction policy.
    epoch: u64,
    families: Vec<String>,
    last: Option<GridBlock>,
    last_run: Option<lumen_text::CachedRun>,
}

#[derive(Clone)]
struct GridBlock {
    lines: Vec<String>,
    size: f32,
}

impl GridBlock {
    fn shape(text: &str, style: &TextStyle, max_width: Option<f32>) -> GridBlock {
        let adv = ADV * style.font_size;
        let per_line = max_width
            .map(|w| ((w / adv).floor() as usize).max(1))
            .unwrap_or(usize::MAX);
        let mut lines = Vec::new();
        let mut cur = String::new();
        for ch in text.chars() {
            if cur.chars().count() >= per_line {
                lines.push(std::mem::take(&mut cur));
            }
            cur.push(ch);
        }
        lines.push(cur);
        GridBlock {
            lines,
            size: style.font_size,
        }
    }
    fn adv(&self) -> f32 {
        ADV * self.size
    }
    fn line_h(&self) -> f32 {
        LINE * self.size
    }
}

impl TextBlockApi for GridBlock {
    fn width(&self) -> f32 {
        self.lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as f32
            * self.adv()
    }
    fn height(&self) -> f32 {
        self.lines.len() as f32 * self.line_h()
    }
    fn size(&self) -> Size {
        Size::new(self.width() as f64, self.height() as f64)
    }
    fn metrics(&self) -> TextMetrics {
        TextMetrics {
            line_count: self.lines.len(),
            box_height: self.height(),
            ascent: self.size * 0.8,
            descent: self.size * 0.2,
            line_height: self.line_h(),
            content_height: self.height(),
        }
    }
    fn missing_glyphs(&self) -> usize {
        0
    }
    fn caret_pos(&self, byte: usize) -> (f32, f32, f32) {
        let mut seen = 0usize;
        for (row, line) in self.lines.iter().enumerate() {
            if seen + line.len() >= byte {
                let col = byte.saturating_sub(seen);
                return (
                    col as f32 * self.adv(),
                    row as f32 * self.line_h(),
                    self.line_h(),
                );
            }
            seen += line.len();
        }
        (self.width(), self.height() - self.line_h(), self.line_h())
    }
    fn hit_to_byte(&self, x: f32, y: f32) -> usize {
        let row = ((y / self.line_h()).floor().max(0.0) as usize).min(self.lines.len() - 1);
        let col = (x / self.adv()).round().max(0.0) as usize;
        let before: usize = self.lines[..row].iter().map(|l| l.len()).sum();
        before + col.min(self.lines[row].len())
    }
    fn selection_rects(&self, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)> {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let (ax, ay, h) = self.caret_pos(a);
        let (bx, by, _) = self.caret_pos(b);
        if (ay - by).abs() < f32::EPSILON {
            vec![(ax, ay, bx - ax, h)]
        } else {
            vec![(ax, ay, self.width() - ax, h), (0.0, by, bx, h)]
        }
    }
    fn render(&self, width: u32, height: u32, background: Color) -> RgbaImage {
        // A solid fill: enough to prove the seam carries rasterization, without
        // this test acquiring a rasterizer.
        let [r, g, b, a] = background.to_srgb8();
        let mut px = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            px.extend_from_slice(&[r, g, b, a]);
        }
        RgbaImage::from_raw(width, height, px)
    }
}

impl TextEngineApi for GridEngine {
    type Block = GridBlock;

    fn begin_frame(&mut self) {
        self.epoch += 1;
    }
    fn register_font(&mut self, bytes: Vec<u8>) -> Option<String> {
        (!bytes.is_empty()).then(|| {
            let name = format!("grid-{}", self.families.len());
            self.families.push(name.clone());
            name
        })
    }
    fn shaped(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        _align: TextAlign,
    ) -> &GridBlock {
        self.last = Some(GridBlock::shape(text, base, max_width));
        self.last.as_ref().unwrap()
    }
    fn shaped_run(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        _align: TextAlign,
        _scale: f32,
    ) -> &lumen_text::CachedRun {
        // A run with no glyphs: this engine has no rasterizer, which is a
        // legitimate implementation (it measures and hit-tests). The point is
        // that the type is CONSTRUCTIBLE from outside the crate — if it were
        // not, the seam would be undeclarable rather than merely unimplemented.
        let block = GridBlock::shape(text, base, max_width);
        self.last_run = Some(lumen_text::CachedRun {
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
        base: TextStyle,
        _ranges: &[(std::ops::Range<usize>, TextStyle)],
        max_width: Option<f32>,
        _align: TextAlign,
    ) -> GridBlock {
        GridBlock::shape(text, &base, max_width)
    }
}

/// The seam is exercised through the TRAIT, never the concrete type — so this
/// function compiles against any implementor, which is the property under test.
fn measure<E: TextEngineApi>(engine: &mut E, text: &str, max_width: Option<f32>) -> (f32, f32) {
    engine.begin_frame();
    let style = TextStyle::default();
    let block = engine.shaped(text, &style, max_width, TextAlign::Start);
    (block.width(), block.height())
}

#[test]
fn a_third_party_engine_satisfies_the_seam() {
    let mut grid = GridEngine::default();
    let (w, h) = measure(&mut grid, "hello", None);
    assert!(w > 0.0 && h > 0.0, "the alternative engine must measure");
    assert_eq!(grid.epoch, 1, "the frame hook must be reachable");
}

#[test]
fn the_bundled_engine_satisfies_the_same_seam() {
    // The generic function above is written once and used for both, which is
    // what makes this a substitutability test rather than two separate ones.
    let mut real = lumen_text::TextEngine::new();
    let (w, h) = measure(&mut real, "hello", None);
    assert!(w > 0.0 && h > 0.0);
}

#[test]
fn wrapping_and_hit_testing_round_trip_through_the_trait() {
    let mut grid = GridEngine::default();
    let style = TextStyle::default();
    grid.begin_frame();
    let block = grid
        .shaped(
            "abcdefghij",
            &style,
            Some(4.0 * ADV * style.font_size),
            TextAlign::Start,
        )
        .clone();
    assert!(block.metrics().line_count > 1, "narrow width must wrap");

    // Hit-testing the caret's own position must return the same byte back.
    let (x, y, _) = block.caret_pos(2);
    assert_eq!(block.hit_to_byte(x, y), 2);

    assert!(!block.selection_rects(0, 6).is_empty());
    assert_eq!(block.render(4, 4, Color::BLACK).pixels().len(), 4 * 4 * 4);
}
