//! `lumen-text` — text shaping, layout, measurement, and CPU rasterization.
//!
//! parley does shaping + layout (bidi, line breaking, alignment); swash does
//! glyph scaling/rasterization (ADR-005). Only the bundled pan-Unicode Noto
//! font is used — never system fonts — so shaping and rasterization are
//! deterministic across machines, which is what makes text goldens trustworthy.
#![warn(missing_docs)]

use kurbo::Size;
use lumen_core::Color;
use lumen_render::RgbaImage;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStack, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

/// Bundled pan-Unicode Noto font (Latin/CJK/Arabic/Hebrew). No system fonts
/// (ADR-005). Color emoji is out of M0 scope; see the decision log.
const FONT: &[u8] = include_bytes!("../fonts/GoNotoKurrent-Regular.ttf");

pub mod editor;
pub mod richtext;
pub use editor::{Preedit, TextEditor};

/// Brush carried through parley to each glyph run: straight sRGB RGBA8.
type Brush = [u8; 4];

// --- per-glyph raster cache (R3.1) ------------------------------------------
//
// Text was rasterized whole-string into a sprite (cached per string in the
// widget layer). That re-rasterizes every glyph whenever a string changes — a
// 1-char edit to an animated readout reshapes and re-renders the whole line.
// Here we cache the swash alpha bitmap per *glyph* (font + id + size + embolden
// + variation coords), so a changed string only rasterizes glyphs it hasn't
// seen. Output is byte-identical (the pen is snapped to whole px, so a glyph's
// bitmap is position-independent), so goldens are unaffected.

/// Identifies a rasterized glyph bitmap. The bundled font is the only face
/// (ADR-005); `font_index`/`data_len` distinguish faces defensively.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font_index: u32,
    data_len: u32,
    glyph_id: u32,
    size_bits: u32,
    embolden_bits: u32,
    coords_hash: u64,
}

/// A cached swash alpha glyph (placement + coverage bitmap).
#[derive(Clone)]
struct CachedGlyph {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Clear the cache above this many glyphs (keeps a long-running session bounded;
/// a full Latin+punctuation set is well under this).
const GLYPH_CACHE_CAP: usize = 8192;

thread_local! {
    static GLYPH_CACHE: RefCell<HashMap<GlyphKey, Option<CachedGlyph>>> =
        RefCell::new(HashMap::new());
    /// Count of actual swash rasterizations (cache misses) — for tests/diagnostics.
    static GLYPH_RASTERS: Cell<u64> = const { Cell::new(0) };
}

fn coords_hash(coords: &[i16]) -> u64 {
    // FNV-1a over the fixed-point variation coords (empty for the static font).
    let mut h = 0xcbf29ce484222325u64;
    for &c in coords {
        h = (h ^ (c as u16 as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
fn reset_glyph_cache() {
    GLYPH_CACHE.with(|c| c.borrow_mut().clear());
    GLYPH_RASTERS.with(|n| n.set(0));
}

#[cfg(test)]
fn glyph_rasters() -> u64 {
    GLYPH_RASTERS.with(|n| n.get())
}

/// Horizontal alignment of wrapped lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Align to the start (left for LTR).
    #[default]
    Start,
    /// Center.
    Center,
    /// Align to the end.
    End,
}

/// A run of text styling.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    /// Font size in logical px.
    pub font_size: f32,
    /// Text color.
    pub color: Color,
    /// Font weight (100–900; 400 = regular, 700 = bold). The bundled font is a
    /// single weight, so heavier values render as synthesized bold.
    pub weight: f32,
    /// Line height as a multiple of font size (`None` = the font's natural
    /// metrics). E.g. `Some(1.4)` for airy body text (B2).
    pub line_height: Option<f32>,
    /// Extra tracking between characters, in logical px (`0.0` = none). Positive
    /// loosens (good for upper-case captions); negative tightens (B2).
    pub letter_spacing: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font_size: 16.0,
            color: Color::BLACK,
            weight: 400.0,
            line_height: None,
            letter_spacing: 0.0,
        }
    }
}

impl TextStyle {
    /// This style at `weight` (e.g. `700.0` for bold).
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// This style with line height set to `multiple` × the font size (B2).
    pub fn line_height(mut self, multiple: f32) -> Self {
        self.line_height = Some(multiple);
        self
    }

    /// This style with `px` of extra letter tracking (B2).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.letter_spacing = px;
        self
    }
}

/// The text engine: owns the bundled-font context. Reuse across layouts.
pub struct TextEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    family: String,
}

impl Default for TextEngine {
    fn default() -> Self {
        TextEngine::new()
    }
}

impl TextEngine {
    /// Build an engine with only the bundled font registered (no system fonts).
    pub fn new() -> TextEngine {
        let mut collection =
            parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            });
        let registered = collection.register_fonts(FONT.to_vec());
        let family = registered
            .first()
            .and_then(|(id, _)| collection.family_name(*id))
            .unwrap_or("Noto")
            .to_string();
        TextEngine {
            font_cx: FontContext {
                collection,
                source_cache: parley::fontique::SourceCache::default(),
            },
            layout_cx: LayoutContext::new(),
            family,
        }
    }

    /// Shape and lay out `text`. `ranges` apply per-byte-range style overrides
    /// (multi-style runs). `max_width` enables wrapping (UAX #14); `None` = no
    /// wrap. Returns a measured, renderable block.
    pub fn layout(
        &mut self,
        text: &str,
        base: TextStyle,
        ranges: &[(std::ops::Range<usize>, TextStyle)],
        max_width: Option<f32>,
        align: TextAlign,
    ) -> TextBlock {
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0);
        builder.push_default(StyleProperty::FontStack(FontStack::Single(
            FontFamily::Named(Cow::Owned(self.family.clone())),
        )));
        builder.push_default(StyleProperty::FontSize(base.font_size));
        builder.push_default(StyleProperty::FontWeight(parley::FontWeight::new(
            base.weight,
        )));
        builder.push_default(StyleProperty::Brush(base.color.to_srgb8()));
        if let Some(lh) = base.line_height {
            builder.push_default(StyleProperty::LineHeight(lh));
        }
        if base.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(base.letter_spacing));
        }
        for (range, style) in ranges {
            builder.push(StyleProperty::FontSize(style.font_size), range.clone());
            builder.push(
                StyleProperty::FontWeight(parley::FontWeight::new(style.weight)),
                range.clone(),
            );
            builder.push(StyleProperty::Brush(style.color.to_srgb8()), range.clone());
        }
        let mut layout: Layout<Brush> = builder.build(text);
        layout.break_all_lines(max_width);
        let parley_align = match align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Middle,
            TextAlign::End => Alignment::End,
        };
        layout.align(max_width, parley_align, AlignmentOptions::default());
        TextBlock { layout }
    }

    /// The x-position (logical px) of byte offset `byte` in `text` at `base`
    /// style, measured by laying out the prefix. Used for selection/caret
    /// geometry (T1.5). `byte` must be a char boundary.
    pub fn measure_prefix(&mut self, text: &str, base: TextStyle, byte: usize) -> f32 {
        if byte == 0 {
            return 0.0;
        }
        self.layout(&text[..byte], base, &[], None, TextAlign::Start)
            .width()
    }

    /// Lay out `text` on a single line, truncating with an ellipsis (`…`) if it
    /// exceeds `max_width` (text-overflow: ellipsis).
    pub fn layout_ellipsized(&mut self, text: &str, base: TextStyle, max_width: f32) -> TextBlock {
        let full = self.layout(text, base, &[], None, TextAlign::Start);
        if full.width() <= max_width {
            return full;
        }
        let ellipsis = '…';
        let mut best = String::from(ellipsis);
        let mut acc = String::new();
        for ch in text.chars() {
            acc.push(ch);
            let candidate = format!("{acc}{ellipsis}");
            if self
                .layout(&candidate, base, &[], None, TextAlign::Start)
                .width()
                <= max_width
            {
                best = candidate;
            } else {
                break;
            }
        }
        self.layout(&best, base, &[], None, TextAlign::Start)
    }
}

/// A laid-out, measured block of text, renderable to an [`RgbaImage`].
pub struct TextBlock {
    layout: Layout<Brush>,
}

impl TextBlock {
    /// The measured width in logical px (stable across runs).
    pub fn width(&self) -> f32 {
        self.layout.width()
    }

    /// The measured height in logical px.
    pub fn height(&self) -> f32 {
        self.layout.height()
    }

    /// The measured size.
    pub fn size(&self) -> Size {
        Size::new(self.width() as f64, self.height() as f64)
    }

    /// The caret geometry for byte offset `byte`: `(x, y, height)` in logical px
    /// (the top-left of a zero-width caret and its line height). Line- and
    /// bidi-aware, so it works for wrapped multi-line text. `byte` is clamped to
    /// the buffer; non-char-boundary offsets snap to the enclosing cluster.
    pub fn caret_pos(&self, byte: usize) -> (f32, f32, f32) {
        use parley::layout::{Affinity, Cursor};
        let cur = Cursor::from_byte_index(&self.layout, byte, Affinity::Downstream);
        let r = cur.geometry(&self.layout, 0.0);
        (r.x0 as f32, r.y0 as f32, (r.y1 - r.y0) as f32)
    }

    /// The byte offset nearest the layout-space point `(x, y)` — the inverse of
    /// [`caret_pos`](Self::caret_pos), for click-to-place / drag-select.
    pub fn hit_to_byte(&self, x: f32, y: f32) -> usize {
        use parley::layout::Cursor;
        Cursor::from_point(&self.layout, x, y).index()
    }

    /// Selection highlight rectangles `(x0, y0, x1, y1)` (logical px) for the
    /// byte range `[a, b)` — one rect per visual line the range spans.
    pub fn selection_rects(&self, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)> {
        use parley::layout::{Affinity, Cursor, Selection};
        let anchor = Cursor::from_byte_index(&self.layout, a, Affinity::Downstream);
        let focus = Cursor::from_byte_index(&self.layout, b, Affinity::Downstream);
        Selection::new(anchor, focus)
            .geometry(&self.layout)
            .into_iter()
            .map(|r| (r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32))
            .collect()
    }

    /// Rasterize onto a `width`×`height` image over `background` (CPU path).
    /// `width`/`height` default to the measured size if zero.
    pub fn render(&self, width: u32, height: u32, background: Color) -> RgbaImage {
        self.render_inner(width, height, background, None)
    }

    /// Like [`TextBlock::render`], but paints a selection highlight from `x0` to
    /// `x1` (logical px) behind the text (T1.5 selection rendering).
    pub fn render_with_selection(
        &self,
        width: u32,
        height: u32,
        background: Color,
        x0: f32,
        x1: f32,
        highlight: Color,
    ) -> RgbaImage {
        self.render_inner(width, height, background, Some((x0, x1, highlight)))
    }

    fn render_inner(
        &self,
        width: u32,
        height: u32,
        background: Color,
        selection: Option<(f32, f32, Color)>,
    ) -> RgbaImage {
        let w = if width == 0 {
            self.width().ceil() as u32
        } else {
            width
        }
        .max(1);
        let h = if height == 0 {
            self.height().ceil() as u32
        } else {
            height
        }
        .max(1);
        let bg = background.to_srgb8();
        let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
        for px in pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&bg);
        }

        // Selection highlight (opaque fill) behind the glyphs.
        if let Some((sx0, sx1, color)) = selection {
            let hc = color.to_srgb8();
            let cx0 = sx0.max(0.0) as u32;
            let cx1 = (sx1.max(0.0) as u32).min(w);
            for y in 0..h {
                for x in cx0..cx1 {
                    let idx = ((y * w + x) * 4) as usize;
                    pixels[idx..idx + 4].copy_from_slice(&hc);
                }
            }
        }

        let mut ctx = ScaleContext::new();
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let color = glyph_run.style().brush;
                let font = run.font();
                let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize)
                else {
                    continue;
                };
                // Faux bold: when the requested weight exceeds the (single)
                // bundled face, parley flags synthesis; embolden the outline.
                // Kept deliberately light (2% of font size): emboldening expands
                // the outline and re-antialiases its edges, so a larger amount
                // visibly softens/blurs bold text. A real bold face would be
                // crisper, but we ship a single weight (ADR-005).
                let strength = if run.synthesis().embolden() {
                    run.font_size() * 0.02
                } else {
                    0.0
                };
                let coords = run.normalized_coords();
                let mut scaler = ctx
                    .builder(font_ref)
                    .size(run.font_size())
                    .hint(true)
                    .normalized_coords(coords)
                    .build();
                let key_base = GlyphKey {
                    font_index: font.index,
                    data_len: font.data.as_ref().len() as u32,
                    glyph_id: 0,
                    size_bits: run.font_size().to_bits(),
                    embolden_bits: strength.to_bits(),
                    coords_hash: coords_hash(coords),
                };
                for glyph in glyph_run.positioned_glyphs() {
                    let key = GlyphKey {
                        glyph_id: glyph.id as u32,
                        ..key_base
                    };
                    GLYPH_CACHE.with(|c| {
                        let mut cache = c.borrow_mut();
                        if cache.len() >= GLYPH_CACHE_CAP && !cache.contains_key(&key) {
                            cache.clear();
                        }
                        let entry = cache.entry(key).or_insert_with(|| {
                            GLYPH_RASTERS.with(|n| n.set(n.get() + 1));
                            let mut render = Render::new(&[Source::Outline]);
                            render.format(Format::Alpha);
                            if strength != 0.0 {
                                render.embolden(strength);
                            }
                            render
                                .render(&mut scaler, glyph.id)
                                .map(|image| CachedGlyph {
                                    left: image.placement.left,
                                    top: image.placement.top,
                                    width: image.placement.width,
                                    height: image.placement.height,
                                    data: image.data,
                                })
                        });
                        if let Some(g) = entry.as_ref() {
                            blit_alpha(&mut pixels, w, h, g, glyph.x, glyph.y, color);
                        }
                    });
                }
            }
        }
        RgbaImage::from_raw(w, h, pixels)
    }
}

/// Composite a cached alpha glyph onto the target at the glyph pen position, in
/// straight-alpha sRGB.
fn blit_alpha(
    pixels: &mut [u8],
    w: u32,
    h: u32,
    g: &CachedGlyph,
    pen_x: f32,
    pen_y: f32,
    color: Brush,
) {
    let gx = pen_x.round() as i32 + g.left;
    let gy = pen_y.round() as i32 - g.top;
    let gw = g.width as i32;
    let gh = g.height as i32;
    for row in 0..gh {
        let py = gy + row;
        if py < 0 || py >= h as i32 {
            continue;
        }
        for col in 0..gw {
            let pxc = gx + col;
            if pxc < 0 || pxc >= w as i32 {
                continue;
            }
            let a = g.data[(row * gw + col) as usize] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let src_a = a * (color[3] as f32 / 255.0);
            let idx = ((py as u32 * w + pxc as u32) * 4) as usize;
            for c in 0..3 {
                let src = color[c] as f32;
                let dst = pixels[idx + c] as f32;
                pixels[idx + c] = (src * src_a + dst * (1.0 - src_a)).round() as u8;
            }
            let dst_a = pixels[idx + 3] as f32 / 255.0;
            pixels[idx + 3] = ((src_a + dst_a * (1.0 - src_a)) * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod glyph_cache_tests {
    //! R3.1: the per-glyph raster cache rasterizes each glyph once and reuses it
    //! across strings, and the cached path is byte-identical to a fresh render.
    use super::*;

    fn style() -> TextStyle {
        TextStyle {
            font_size: 24.0,
            color: Color::srgb8(0, 0, 0, 255),
            weight: 400.0,
            line_height: None,
            letter_spacing: 0.0,
        }
    }

    fn render_str(te: &mut TextEngine, s: &str) -> RgbaImage {
        let block = te.layout(s, style(), &[], None, TextAlign::Start);
        block.render(0, 0, Color::srgb8(255, 255, 255, 0))
    }

    #[test]
    fn only_new_glyphs_are_rasterized() {
        reset_glyph_cache();
        let mut te = TextEngine::new();

        render_str(&mut te, "abc");
        let after_abc = glyph_rasters();
        assert_eq!(
            after_abc, 3,
            "three distinct glyphs (a, b, c) rasterized once"
        );

        // A 1-character extension rasterizes only the new glyph.
        render_str(&mut te, "abcd");
        assert_eq!(glyph_rasters(), after_abc + 1, "only 'd' is new");

        // Re-rendering already-seen glyphs (reordered) rasterizes nothing.
        render_str(&mut te, "cab");
        assert_eq!(glyph_rasters(), after_abc + 1, "all glyphs already cached");
    }

    #[test]
    fn cached_render_is_byte_identical() {
        reset_glyph_cache();
        let mut te = TextEngine::new();
        let first = render_str(&mut te, "Hello, world");
        // Second render hits the glyph cache for every glyph.
        let cached = render_str(&mut te, "Hello, world");
        assert_eq!(
            first.pixels(),
            cached.pixels(),
            "the cached glyph path must be byte-identical"
        );
    }
}
