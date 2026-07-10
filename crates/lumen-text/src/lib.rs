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
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, Layout, LayoutContext,
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

impl GlyphKey {
    /// A stable 64-bit identity for cross-frame atlas caching (FNV-1a over the
    /// fields). Deterministic; only used as a cache key, never to render.
    fn stable_id(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for word in [
            self.font_index as u64,
            self.data_len as u64,
            self.glyph_id as u64,
            self.size_bits as u64,
            self.embolden_bits as u64,
            self.coords_hash,
        ] {
            h = (h ^ word).wrapping_mul(0x100000001b3);
        }
        h
    }
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
#[derive(Clone, Debug)]
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
    /// Font family to shape with (`None` = the engine default, the bundled font).
    /// Register custom fonts via [`TextEngine::register_font`]; select by the
    /// returned family name (B1, no system enumeration).
    pub family: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font_size: 16.0,
            color: Color::BLACK,
            weight: 400.0,
            line_height: None,
            letter_spacing: 0.0,
            family: None,
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

    /// This style shaped with the named font `family` (as returned by
    /// [`TextEngine::register_font`]); `None`/unset uses the engine default.
    pub fn family(mut self, name: impl Into<String>) -> Self {
        self.family = Some(name.into());
        self
    }
}

/// Cache key for a shaped [`TextBlock`] — the **geometry-affecting** style only.
/// Color is excluded (it doesn't affect shaping/metrics; the glyph run applies it
/// at emission), so measure and a `.lss`-recolored paint share one entry.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ShapeKey {
    text: String,
    size: u32,
    weight: u32,
    line_height: Option<u32>,
    letter_spacing: u32,
    family: Option<String>,
    wrap: Option<u32>,
    align: u8,
}

impl ShapeKey {
    fn new(text: &str, s: &TextStyle, wrap: Option<f32>, align: TextAlign) -> ShapeKey {
        ShapeKey {
            text: text.to_string(),
            size: s.font_size.to_bits(),
            weight: s.weight.to_bits(),
            line_height: s.line_height.map(f32::to_bits),
            letter_spacing: s.letter_spacing.to_bits(),
            family: s.family.clone(),
            wrap: wrap.map(f32::to_bits),
            align: align as u8,
        }
    }
}

/// Clear the shaped-layout cache above this many entries (bounds a long session
/// with many distinct strings, e.g. an animated numeric readout).
const SHAPE_CACHE_CAP: usize = 2048;
/// Cap for the glyph-run cache (R5). Same rationale as the shape cache.
const RUN_CACHE_CAP: usize = 4096;

/// A cached, **origin-relative** glyph run (R5 incremental paint): the positioned
/// glyphs (laid out at origin 0,0), their coverage images, the ink bounds
/// `[x0,y0,x1,y1]`, and metrics. The paint layer interns the images into the
/// frame and translates the run by the node's origin — so a static (or merely
/// scrolled) label reuses this instead of re-running `glyph_run` (the dominant
/// display-list-emission cost). Byte-identical to building at the origin
/// directly, because `glyph_run` rounds the pen *before* adding the origin.
pub struct CachedRun {
    /// Glyphs positioned relative to origin (0, 0).
    pub run: lumen_render::GlyphRun,
    /// Coverage images referenced by the run (local indices).
    pub images: Vec<lumen_render::GlyphImage>,
    /// Ink bounds `[x0, y0, x1, y1]`, origin-relative.
    pub ink: [f32; 4],
    /// Typographic metrics (position-independent).
    pub metrics: TextMetrics,
}

/// The text engine: owns the bundled-font context. Reuse across layouts.
pub struct TextEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    family: String,
    /// Cache of shaped blocks keyed by geometry-affecting style. parley shaping
    /// is the dominant per-frame cost; the runtime shapes each label both to
    /// measure it and to paint it, every frame — this collapses that to one
    /// shaping per `(text, geometry, wrap)` and reuses it across frames.
    shape_cache: HashMap<ShapeKey, TextBlock>,
    /// Cache of origin-relative glyph runs keyed by `(ShapeKey, scale)` (R5). The
    /// paint layer translates + interns these instead of re-building the run each
    /// frame — the dominant display-list-emission cost for text.
    run_cache: HashMap<(ShapeKey, u32), CachedRun>,
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
        // Wrap the embedded font in a Blob over the `&'static` slice — no heap
        // copy. fontique 0.11 takes an `Arc`-backed `Blob`, so the 15 MB bundled
        // font is referenced in place rather than duplicated on the heap (the old
        // `FONT.to_vec()` doubled its resident footprint).
        let blob = parley::fontique::Blob::new(std::sync::Arc::new(FONT));
        let registered = collection.register_fonts(blob, None);
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
            shape_cache: HashMap::new(),
            run_cache: HashMap::new(),
        }
    }

    /// Register an additional font from its `bytes` and return its family name —
    /// pass that name to [`TextStyle::family`] to shape with it. Additive and
    /// explicit (the app provides the bytes): no system-font enumeration, so
    /// shaping stays deterministic (ADR-005). Returns `None` if the bytes don't
    /// parse as a font. The bundled font remains the default.
    pub fn register_font(&mut self, bytes: Vec<u8>) -> Option<String> {
        let registered = self
            .font_cx
            .collection
            .register_fonts(parley::fontique::Blob::from(bytes), None);
        // A new family can change fallback shaping for any string, so drop cached
        // shaped blocks + runs.
        self.shape_cache.clear();
        self.run_cache.clear();
        let (id, _) = registered.first()?;
        self.font_cx
            .collection
            .family_name(*id)
            .map(|s| s.to_string())
    }

    /// Shape `text` at `base` style with optional `max_width` wrap and `align`,
    /// returning a cached [`TextBlock`] (single-style runs only). The runtime
    /// shapes each label to measure *and* to paint it every frame; this collapses
    /// that to one parley shaping per `(text, geometry, wrap, align)` and reuses
    /// it across frames. Color is not part of the key (it's applied at glyph-run
    /// emission), so a `.lss` recolor still hits the same entry. For per-range
    /// styles or color-baked rasterization, call [`layout`](Self::layout).
    pub fn shaped(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> &TextBlock {
        let key = ShapeKey::new(text, base, max_width, align);
        if !self.shape_cache.contains_key(&key) {
            let block = self.layout(text, base.clone(), &[], max_width, align);
            if self.shape_cache.len() >= SHAPE_CACHE_CAP {
                // R.5: drop ~half instead of everything — a cap crossing
                // costs one half-refill, not a full re-shape stall. Iteration
                // order is arbitrary but caches are output-transparent.
                let mut keep = self.shape_cache.len() / 2;
                self.shape_cache.retain(|_, _| {
                    let k = keep > 0;
                    keep = keep.saturating_sub(1);
                    k
                });
            }
            self.shape_cache.insert(key.clone(), block);
        }
        &self.shape_cache[&key]
    }

    /// Like [`shaped`](Self::shaped) but returns the **origin-relative glyph run**
    /// (R5): positioned glyphs, coverage images, ink, and metrics, cached by
    /// `(ShapeKey, scale)`. The paint layer translates + interns it, skipping the
    /// per-frame `glyph_run` rebuild for static/scrolled text.
    pub fn shaped_run(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
        scale: f32,
    ) -> &CachedRun {
        let key = (ShapeKey::new(text, base, max_width, align), scale.to_bits());
        if !self.run_cache.contains_key(&key) {
            let cached = {
                let block = self.shaped(text, base, max_width, align);
                let (run, images) = block.glyph_run(0.0, 0.0, scale);
                // Origin-relative ink; starts at the origin (0,0) like the paint
                // layer's `run_rect`, then unions each glyph.
                let mut ink = [0f32; 4];
                for g in &run.glyphs {
                    ink[0] = ink[0].min(g.x);
                    ink[1] = ink[1].min(g.y);
                    ink[2] = ink[2].max(g.x + g.w);
                    ink[3] = ink[3].max(g.y + g.h);
                }
                CachedRun {
                    run,
                    images,
                    ink,
                    metrics: block.metrics(),
                }
            };
            if self.run_cache.len() >= RUN_CACHE_CAP {
                // R.5: drop ~half instead of everything — a cap crossing
                // costs one half-refill, not a full re-shape stall. Iteration
                // order is arbitrary but caches are output-transparent.
                let mut keep = self.run_cache.len() / 2;
                self.run_cache.retain(|_, _| {
                    let k = keep > 0;
                    keep = keep.saturating_sub(1);
                    k
                });
            }
            self.run_cache.insert(key.clone(), cached);
        }
        &self.run_cache[&key]
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
        // Resolve families to known registered ones *before* borrowing the font
        // context for the builder; an unknown name falls back to the engine
        // default (the bundled font). With no system fonts an unmatched family
        // would otherwise shape nothing.
        let resolve_family = |cx: &mut FontContext, want: &Option<String>| -> String {
            match want {
                Some(n) if cx.collection.family_id(n).is_some() => n.clone(),
                _ => self.family.clone(),
            }
        };
        let default_family = resolve_family(&mut self.font_cx, &base.family);
        let range_families: Vec<Option<String>> = ranges
            .iter()
            .map(|(_, style)| {
                style
                    .family
                    .as_ref()
                    .filter(|n| self.font_cx.collection.family_id(n).is_some())
                    .cloned()
            })
            .collect();

        // `quantize: false` keeps fractional logical-px layout — we rasterize at
        // physical scale separately (for_each_glyph), so snapping positions to the
        // logical grid here would coarsen HiDPI text.
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Named(Cow::Owned(default_family)),
        )));
        builder.push_default(StyleProperty::FontSize(base.font_size));
        builder.push_default(StyleProperty::FontWeight(parley::FontWeight::new(
            base.weight,
        )));
        builder.push_default(StyleProperty::Brush(base.color.to_srgb8()));
        // Line height as a multiple of font size. parley's low-level builder
        // defaults to 1.0, which is too tight for this font — ascenders/descenders
        // (g, y, p, q, accents) fall outside the line box and get clipped when the
        // run is rasterized to its measured height. Default to 1.3 (a touch above
        // the ~1.25 where the bundled font's full glyph extent fits at every size)
        // so the box always reserves room for the whole glyph.
        // parley 0.11's LineHeight is an enum; FontSizeRelative matches the old
        // f32-multiple-of-font-size semantics (its own default is now
        // MetricsRelative, which would change spacing).
        builder.push_default(StyleProperty::LineHeight(
            parley::LineHeight::FontSizeRelative(base.line_height.unwrap_or(1.3)),
        ));
        if base.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(base.letter_spacing));
        }
        for (i, (range, style)) in ranges.iter().enumerate() {
            builder.push(StyleProperty::FontSize(style.font_size), range.clone());
            builder.push(
                StyleProperty::FontWeight(parley::FontWeight::new(style.weight)),
                range.clone(),
            );
            builder.push(StyleProperty::Brush(style.color.to_srgb8()), range.clone());
            if let Some(fam) = &range_families[i] {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::Single(FontFamilyName::Named(
                        Cow::Owned(fam.clone()),
                    ))),
                    range.clone(),
                );
            }
        }
        let mut layout: Layout<Brush> = builder.build(text);
        layout.break_all_lines(max_width);
        let parley_align = match align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
        };
        // `max_width` was already applied via break_all_lines; align() now takes
        // just (alignment, options).
        layout.align(parley_align, AlignmentOptions::default());
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
        let full = self.layout(text, base.clone(), &[], None, TextAlign::Start);
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
                .layout(&candidate, base.clone(), &[], None, TextAlign::Start)
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

/// Typographic metrics for a laid-out [`TextBlock`] — a diagnostic aid that
/// names the line-height class of clipping (`content_height > box_height`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    /// Number of (wrapped) lines.
    pub line_count: usize,
    /// The reserved block height (logical px) — [`TextBlock::height`].
    pub box_height: f32,
    /// Max typographic ascent across lines (logical px).
    pub ascent: f32,
    /// Max typographic descent across lines (logical px).
    pub descent: f32,
    /// Max per-line box height across lines (logical px).
    pub line_height: f32,
    /// Sum of each line's ascent+descent — the actual glyph extent. Exceeding
    /// `box_height` means the line boxes are too short and glyphs are clipped.
    pub content_height: f32,
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

    /// Typographic metrics for the laid-out block (diagnostic aid). `box_height`
    /// is the reserved height ([`height`](Self::height)); `content_height` is the
    /// sum of each line's *declared* ascent+descent. `content_height > box_height`
    /// means the line-height is tighter than the font's declared extent — a hint,
    /// not proof of clipping (actual glyph ink is usually tighter than the
    /// declared metrics). The authoritative clip check is the rendered ink bounds
    /// (`SemanticsNode.ink` / the W0104 audit).
    pub fn metrics(&self) -> TextMetrics {
        let mut m = TextMetrics {
            line_count: 0,
            box_height: self.layout.height(),
            ascent: 0.0,
            descent: 0.0,
            line_height: 0.0,
            content_height: 0.0,
        };
        for line in self.layout.lines() {
            let lm = line.metrics();
            m.line_count += 1;
            m.ascent = m.ascent.max(lm.ascent);
            m.descent = m.descent.max(lm.descent);
            m.line_height = m.line_height.max(lm.line_height);
            m.content_height += lm.ascent + lm.descent;
        }
        m
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
            // geometry() now yields (BoundingBox, line_index) pairs.
            .map(|(r, _)| (r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32))
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

        self.for_each_glyph(1.0, |_key, g, pen_x, pen_y, color| {
            blit_alpha(&mut pixels, w, h, g, pen_x, pen_y, color);
        });
        RgbaImage::from_raw(w, h, pixels)
    }

    /// Walk every laid-out glyph, rasterizing it (or hitting the per-glyph cache)
    /// at `scale`× the logical font size, and call `f(key, bitmap, pen_x, pen_y,
    /// color)` — `pen_x`/`pen_y` are logical, the bitmap is physical-resolution.
    /// Shared by the sprite renderer (`scale = 1.0`) and the [`glyph_run`]
    /// producer (HiDPI scale) so both see identical rasterization.
    fn for_each_glyph(
        &self,
        scale: f32,
        mut f: impl FnMut(GlyphKey, &CachedGlyph, f32, f32, Brush),
    ) {
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
                // Rasterize at the physical size (logical × scale) so HiDPI text
                // is crisp; the key buckets by physical size, so 1× and 2× of the
                // same glyph are distinct atlas entries (R3.5).
                let phys_size = run.font_size() * scale;
                let strength = if run.synthesis().embolden() {
                    phys_size * 0.02
                } else {
                    0.0
                };
                let coords = run.normalized_coords();
                let mut scaler = ctx
                    .builder(font_ref)
                    .size(phys_size)
                    .hint(true)
                    .normalized_coords(coords)
                    .build();
                let key_base = GlyphKey {
                    font_index: font.index,
                    data_len: font.data.as_ref().len() as u32,
                    glyph_id: 0,
                    size_bits: phys_size.to_bits(),
                    embolden_bits: strength.to_bits(),
                    coords_hash: coords_hash(coords),
                };
                for glyph in glyph_run.positioned_glyphs() {
                    let key = GlyphKey {
                        glyph_id: glyph.id,
                        ..key_base
                    };
                    GLYPH_CACHE.with(|c| {
                        let mut cache = c.borrow_mut();
                        if cache.len() >= GLYPH_CACHE_CAP && !cache.contains_key(&key) {
                            // R.5: half-retention, not a full re-raster stall.
                            let mut keep = cache.len() / 2;
                            cache.retain(|_, _| {
                                let k = keep > 0;
                                keep = keep.saturating_sub(1);
                                k
                            });
                        }
                        let entry = cache.entry(key).or_insert_with(|| {
                            GLYPH_RASTERS.with(|n| n.set(n.get() + 1));
                            let mut render = Render::new(&[Source::Outline]);
                            render.format(Format::Alpha);
                            if strength != 0.0 {
                                render.embolden(strength);
                            }
                            render
                                .render(&mut scaler, glyph.id as u16)
                                .map(|image| CachedGlyph {
                                    left: image.placement.left,
                                    top: image.placement.top,
                                    width: image.placement.width,
                                    height: image.placement.height,
                                    data: image.data,
                                })
                        });
                        if let Some(g) = entry.as_ref() {
                            f(key, g, glyph.x, glyph.y, color);
                        }
                    });
                }
            }
        }
    }

    /// Produce a renderer-ready glyph run for the GPU/CPU `DrawCmd::GlyphRun`
    /// path (R3): positioned glyphs plus their deduplicated coverage bitmaps,
    /// translated to window origin `(ox, oy)`. Glyphs are rasterized at `scale`×
    /// the logical font size (HiDPI crispness, R3.5); the placed glyph's dest
    /// rect is in logical px (bitmap size ÷ scale). `scale == 1.0` reproduces the
    /// sprite path exactly. Reuses the per-glyph raster cache. The run's color is
    /// uniform and set by the caller on the `DrawCmd` (multi-color text still uses
    /// the sprite path for now).
    pub fn glyph_run(
        &self,
        ox: f32,
        oy: f32,
        scale: f32,
    ) -> (lumen_render::GlyphRun, Vec<lumen_render::GlyphImage>) {
        let mut images: Vec<lumen_render::GlyphImage> = Vec::new();
        let mut glyphs: Vec<lumen_render::PlacedGlyph> = Vec::new();
        self.for_each_glyph(scale, |key, g, pen_x, pen_y, _color| {
            if g.width == 0 || g.height == 0 {
                return; // whitespace — nothing to paint
            }
            let id = key.stable_id();
            let image = match images.iter().position(|gi| gi.key == id) {
                Some(i) => i as u32,
                None => {
                    images.push(lumen_render::GlyphImage {
                        key: id,
                        width: g.width,
                        height: g.height,
                        coverage: g.data.clone(),
                    });
                    (images.len() - 1) as u32
                }
            };
            // The pen rounds in logical px (stable across scales); the physical
            // bearings/size convert back to logical for the dest rect.
            glyphs.push(lumen_render::PlacedGlyph {
                image,
                x: ox + pen_x.round() + g.left as f32 / scale,
                y: oy + pen_y.round() - g.top as f32 / scale,
                w: g.width as f32 / scale,
                h: g.height as f32 / scale,
            });
        });
        (lumen_render::GlyphRun { glyphs }, images)
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
            // Straight-alpha source-over. The destination's RGB is weighted by
            // *its own* alpha, so compositing onto a transparent pixel yields the
            // source color (not a blend toward the buffer's fill color) — that's
            // what keeps glyph AA edges the right darkness over any background.
            let dst_a = pixels[idx + 3] as f32 / 255.0;
            let out_a = src_a + dst_a * (1.0 - src_a);
            if out_a > 0.0 {
                for c in 0..3 {
                    let src = color[c] as f32;
                    let dst = pixels[idx + c] as f32;
                    let out = (src * src_a + dst * dst_a * (1.0 - src_a)) / out_a;
                    pixels[idx + c] = out.round() as u8;
                }
            }
            pixels[idx + 3] = (out_a * 255.0).round() as u8;
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
            family: None,
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

#[cfg(test)]
mod eviction_tests {
    use super::*;

    /// R.5: crossing the shape-cache cap retains ~half the entries instead
    /// of clearing — the hot working set partially survives, so a crossing
    /// costs one half-refill rather than a full re-shape stall.
    #[test]
    fn shape_cache_overflow_keeps_half() {
        let mut engine = TextEngine::new();
        let style = TextStyle::default();
        for i in 0..SHAPE_CACHE_CAP {
            engine.shaped(&format!("s{i}"), &style, None, TextAlign::Start);
        }
        assert_eq!(engine.shape_cache.len(), SHAPE_CACHE_CAP);
        // The insert that crosses the cap halves the cache first.
        engine.shaped("overflow", &style, None, TextAlign::Start);
        let len = engine.shape_cache.len();
        assert!(
            len > SHAPE_CACHE_CAP / 4 && len <= SHAPE_CACHE_CAP / 2 + 1,
            "half retained, got {len} of {SHAPE_CACHE_CAP}"
        );
    }
}
