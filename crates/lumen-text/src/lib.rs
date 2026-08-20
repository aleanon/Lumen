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
// R1: these caches are keyed on text and glyph descriptors the app itself
// mints, re-probed every frame, and never adversarial — std's SipHash-1-3 is
// the wrong trade. `sip::Hasher::write` was 5.4% of a 3000-row frame and the
// swap measured 4.4% (docs/profile-vs-iced-2026-08-19.md). Shadowing the std
// name keeps the declarations below unchanged; only `new()` becomes
// `default()`, which std reserves for `RandomState`.
use lumen_core::fxhash::HashMap;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

/// Bundled pan-Unicode Noto font (Latin/CJK/Arabic/Hebrew). No system fonts
/// (ADR-005). Color emoji is out of M0 scope; see the decision log.
// T.4: the default face. Pan-Unicode under the default `pan-unicode`
// feature (CJK/RTL/Indic coverage for the goldens + i18n examples); the
// lean build embeds the ~350 KB Latin+symbols subset (chevrons, arrows,
// checkmarks — everything the built-in widgets draw) and apps register a
// wider face at runtime if they need one.
#[cfg(feature = "pan-unicode")]
const FONT: &[u8] = include_bytes!("../fonts/GoNotoKurrent-Regular.ttf");
#[cfg(not(feature = "pan-unicode"))]
const FONT: &[u8] = include_bytes!("../fonts/GoNotoKurrent-Latin.ttf");
// T.4: symbols fallback — Go Noto Kurrent has no geometric shapes / arrows /
// stars (the accordion chevron was tofu!), so a ~170 KB DejaVu Sans subset
// (license in fonts/LICENSE-DejaVu) rides along in every build and registers
// as a fallback face.
//
// LN1: both derived faces are byte-for-byte reproducible via
// `scripts/subset_fonts.sh` (CI job `fonts`). The exact coverage lives there
// as data, not prose — this comment used to say "U+2000–2BFF symbol blocks",
// but the real coverage is 45 discrete ranges reaching U+FFFD, so that
// description could never have re-cut the file.
const SYMBOLS_FONT: &[u8] = include_bytes!("../fonts/DejaVuSans-Symbols.ttf");

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
    skew_bits: u32,
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
            self.skew_bits as u64,
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
        RefCell::new(HashMap::default());
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
    /// OpenType feature settings in CSS `font-feature-settings` syntax
    /// (PROP1) — e.g. `"smcp" 1, "tnum" 1`. Passed to the shaper verbatim;
    /// tags the face does not carry are ignored by it, as in CSS.
    pub features: Option<String>,
    /// Variable-font axis settings in CSS `font-variation-settings` syntax
    /// (PROP1) — e.g. `"wght" 700`.
    ///
    /// The bundled face is **static** (no `fvar` axes), so this has no visible
    /// effect until an app registers a variable face with
    /// [`TextEngine::register_font`] — exactly like `family`.
    pub variations: Option<String>,
    /// Render italic (PROP1). The bundled face ships one upright style
    /// (ADR-005), so this is satisfied by **synthetic oblique** — the same
    /// route the existing faux-bold takes for weight.
    pub italic: bool,
    /// Horizontal alignment of wrapped lines (PROP1).
    ///
    /// Carried on the style rather than passed per call so a `.lss`
    /// `text-align` can reach the shaper: the runtime had `TextAlign::Start`
    /// hardcoded at nine call sites, which is why the property was inert.
    /// `shaped`/`layout` still TAKE an align argument — it is part of the cache
    /// key — and callers pass this field.
    pub align: TextAlign,
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
            features: None,
            variations: None,
            italic: false,
            align: TextAlign::Start,
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

    /// This style with OpenType `features` (PROP1), CSS
    /// `font-feature-settings` syntax.
    pub fn features(mut self, settings: impl Into<String>) -> Self {
        self.features = Some(settings.into());
        self
    }

    /// This style with variable-font `settings` (PROP1), CSS
    /// `font-variation-settings` syntax.
    pub fn variations(mut self, settings: impl Into<String>) -> Self {
        self.variations = Some(settings.into());
        self
    }

    /// This style rendered italic (PROP1) — synthetic oblique, see
    /// [`TextStyle::italic`].
    pub fn italic(mut self, yes: bool) -> Self {
        self.italic = yes;
        self
    }

    /// This style with wrapped lines aligned to `align` (PROP1).
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
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
///
/// # R2: a 128-bit content hash, not the content
///
/// This used to be a struct owning the full `String` plus three
/// `Option<String>`s. Every lookup therefore allocated the key it was about to
/// hash (`text.to_string()`), and every HIT compared the whole string again —
/// `ShapeKey::eq` 4.3% of a 3000-row frame and `__memcmp_avx2` 2.9%, on top of
/// the allocator traffic (`docs/profile-vs-iced-2026-08-19.md`).
///
/// Now the key IS the hash. Lookups allocate nothing, `eq` compares 16 bytes,
/// and the map hashes 16 bytes instead of the whole string.
///
/// **Collision policy** is ADR-021's, and deliberately the same argument: 128
/// bits is chosen so no collision probe is needed. `IdHasher`'s two
/// independent lanes are full-entropy by construction, so at this cache's hard
/// cap of 16 384 entries the birthday probability is ~1e-30. A collision here
/// would render one string with another's shaping — visible, wrong, and
/// strictly less severe than the snapshot corruption ADR-021 accepts the same
/// risk for.
///
/// Every field that fed the old struct still feeds the hash; the PROP1 notes
/// below record why each is load-bearing.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ShapeKey(lumen_core::identity::IdHash);

impl ShapeKey {
    fn new(text: &str, s: &TextStyle, wrap: Option<f32>, align: TextAlign) -> ShapeKey {
        // Borrowed throughout — nothing here allocates.
        //
        // `variations` (PROP1): axis settings change the OUTLINES a face
        // produces, so like `features` they must key the cache.
        // `features` (PROP1): feature settings change glyph SELECTION (`smcp`
        // substitutes small caps), so like `italic` they must key the cache or
        // differently-featured runs of one string collide.
        // `italic` (PROP1): load-bearing for CORRECTNESS, not completeness —
        // it changes which face parley selects and therefore the synthesis
        // flag the shaped run carries. Omitted, the first block shaped for a
        // given string wins and every later node with a different `font-style`
        // silently reuses it.
        ShapeKey(lumen_core::identity::hash_id(&(
            text,
            s.font_size.to_bits(),
            s.weight.to_bits(),
            s.line_height.map(f32::to_bits),
            s.letter_spacing.to_bits(),
            s.family.as_deref(),
            wrap.map(f32::to_bits),
            align as u8,
            s.variations.as_deref(),
            s.features.as_deref(),
            s.italic,
        )))
    }
}

/// Sweep the shaped-layout cache above this many entries (bounds a long session
/// with many distinct strings, e.g. an animated numeric readout).
///
/// This is a *soft* cap: crossing it triggers [`sweep`], which drops entries not
/// used in the last two frames. If the live working set alone exceeds it, the
/// cap is retargeted at that set rather than evicting entries the next frame
/// will immediately re-shape — see [`sweep`] for why.
const SHAPE_CACHE_CAP: usize = 2048;
/// Soft cap for the glyph-run cache (R5). Same rationale as the shape cache.
const RUN_CACHE_CAP: usize = 4096;
/// Absolute ceilings. Past these the live working set is larger than any cache
/// should hold (a non-virtualized list of tens of thousands of distinct labels),
/// so we stop growing and accept eviction; `VirtualList` is the answer there,
/// not more memory.
const SHAPE_CACHE_HARD_CAP: usize = 16_384;
/// Absolute ceiling for the glyph-run cache. See [`SHAPE_CACHE_HARD_CAP`].
const RUN_CACHE_HARD_CAP: usize = 32_768;

/// A cache entry tagged with the frame epoch it was last used in.
struct Aged<V> {
    value: V,
    epoch: u64,
}

/// Reclaim a cache that has crossed its soft cap, then retarget the cap.
///
/// # Why not simply drop half
///
/// The previous policy retained an arbitrary half (`retain` over hash order,
/// carrying no recency information). That is sound only while the live working
/// set fits in `cap / 2`. Past that it is self-sustaining: the sweep drops to
/// `cap / 2`, the same frame re-shapes the entries it still needs, refilling
/// past `cap` again — so a *single* crossing locks the cache into thrashing on
/// every subsequent frame, permanently.
///
/// Measured on an N-row list (`benches-competitive`, 400 frames): at 2000 rows
/// this cost **1183 re-shapes per frame** and 1.16 evictions per frame, for a
/// 2.2x frame-time penalty (3.8 ms → 8.5 ms). The trigger is cumulative distinct
/// strings crossing `cap`; the lock-in condition is a live set above `cap / 2`.
/// One changing label (a clock, a counter) is enough to drive any app with more
/// than ~1024 distinct strings there, given enough frames — 1400 rows measured
/// clean only because it had not crossed *yet*.
///
/// # The policy
///
/// Keep entries used **this frame or last**, drop the rest. "Or last" is
/// load-bearing: a crossing happens mid-frame, so entries not yet reached this
/// frame still carry the previous epoch, and dropping them would re-shape them
/// moments later — reintroducing the sequential-scan worst case that defeats
/// plain LRU. Stale strings (the growth source) are exactly what this reclaims.
fn sweep<K: Eq + std::hash::Hash, V, S: std::hash::BuildHasher>(
    map: &mut std::collections::HashMap<K, Aged<V>, S>,
    epoch: u64,
    cap: &mut usize,
    base_cap: usize,
    hard_cap: usize,
) {
    map.retain(|_, e| e.epoch + 1 >= epoch);
    if map.len() >= hard_cap {
        // The live set alone is over the ceiling: fall back to dropping half.
        let mut keep = map.len() / 2;
        map.retain(|_, _| {
            let k = keep > 0;
            keep = keep.saturating_sub(1);
            k
        });
    }
    // Retarget at the live set, so the next crossing is a frame away rather than
    // an insert away (which would make every insert an O(n) scan). Shrinks back
    // toward the base cap as the working set does.
    *cap = base_cap.max(map.len().saturating_mul(2)).min(hard_cap);
}

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

/// MOD3: the text seam — shaping, measurement and rasterization behind a trait,
/// so an alternative text stack can be supplied without forking.
///
/// Like the layout seam (MOD2), the surface is deliberately **narrow**: it is
/// derived from what `lumen-widgets` actually calls, not from everything
/// [`TextEngine`] exposes. Mirroring the whole inherent API would force an
/// implementor to reproduce methods the runtime never uses — `outlines`,
/// `render_with_selection` and `layout_ellipsized` have zero call sites in the
/// runtime, and baking them into the seam would tax every implementor for
/// features only the bundled engine offers.
///
/// `begin_frame` IS included despite being a cache-lifecycle hook rather than a
/// text operation: any implementation with a cache needs a frame boundary to
/// evict against, and leaving it out would force each one to invent its own
/// (the alternative — inferring frames from call patterns — is exactly the bug
/// the epoch policy replaced). An engine with no cache implements it empty.
pub trait TextEngineApi {
    /// The shaped block this engine produces.
    type Block: TextBlockApi;

    /// Advance the frame epoch. Called once per frame that shapes; see
    /// [`TextEngine::begin_frame`] for why it must not be called when idle.
    fn begin_frame(&mut self);

    /// Register a font, returning its family name.
    fn register_font(&mut self, bytes: Vec<u8>) -> Option<String>;

    /// Shape (or fetch a cached) block for `text` under `base`.
    fn shaped(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> &Self::Block;

    /// The origin-relative glyph run for `text` (R5): the paint fast path.
    ///
    /// This is in the seam because the runtime's paint pass calls it — a seam
    /// without it would cover measurement and hit-testing but not the path that
    /// actually emits glyphs, i.e. it could not stand in for this engine. It
    /// does mean an implementor must produce `lumen_render` glyph types, which
    /// is the honest cost of substituting a text stack in a framework whose
    /// renderer consumes positioned glyphs.
    fn shaped_run(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
        scale: f32,
    ) -> &CachedRun;

    /// Lay out a block with per-range styles, bypassing the cache.
    fn layout(
        &mut self,
        text: &str,
        base: TextStyle,
        ranges: &[(std::ops::Range<usize>, TextStyle)],
        max_width: Option<f32>,
        align: TextAlign,
    ) -> Self::Block;

    /// The truncated string to PAINT for `text-overflow: ellipsis`, or `None` if
    /// `text` already fits (PROP1).
    ///
    /// A **provided** method, not a required one: truncation is just repeated
    /// measurement, so it is expressed in terms of `layout` and every
    /// implementor gets it free. The seam stays as narrow as MOD3 made it.
    ///
    /// It returns the *string* rather than a laid-out block because the runtime
    /// paints it while the node keeps its FULL text — the semantic tree, the
    /// agent and assistive tech must not see "Some long lab…". That split is the
    /// whole reason this property needed more than a bridge.
    fn ellipsized_text(&mut self, text: &str, base: &TextStyle, max_width: f32) -> Option<String> {
        if self
            .layout(text, base.clone(), &[], None, base.align)
            .width()
            <= max_width
        {
            return None;
        }
        let ellipsis = '…';
        let mut best = String::from(ellipsis);
        let mut acc = String::new();
        for ch in text.chars() {
            acc.push(ch);
            let candidate = format!("{acc}{ellipsis}");
            if self
                .layout(&candidate, base.clone(), &[], None, base.align)
                .width()
                <= max_width
            {
                best = candidate;
            } else {
                break;
            }
        }
        Some(best)
    }
}

/// The measurement + hit-testing surface of a shaped block (MOD3).
///
/// `render` is here rather than on the engine because rasterization depends on
/// the shaped result, and an engine that shapes but cannot rasterize is not a
/// substitute for this one.
pub trait TextBlockApi {
    /// Advance width of the widest line.
    fn width(&self) -> f32;
    /// Total laid-out height.
    fn height(&self) -> f32;
    /// `(width, height)`.
    fn size(&self) -> Size;
    /// Typographic metrics.
    fn metrics(&self) -> TextMetrics;
    /// Glyphs that fell back to `.notdef` — the tofu detector.
    fn missing_glyphs(&self) -> usize;
    /// Caret `(x, y, height)` at a byte offset.
    fn caret_pos(&self, byte: usize) -> (f32, f32, f32);
    /// Byte offset nearest a point.
    fn hit_to_byte(&self, x: f32, y: f32) -> usize;
    /// Selection rectangles `(x, y, w, h)` between two byte offsets.
    fn selection_rects(&self, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)>;
    /// Rasterize onto `background`.
    fn render(&self, width: u32, height: u32, background: Color) -> RgbaImage;
}

impl TextEngineApi for TextEngine {
    type Block = TextBlock;

    fn begin_frame(&mut self) {
        TextEngine::begin_frame(self)
    }
    fn register_font(&mut self, bytes: Vec<u8>) -> Option<String> {
        TextEngine::register_font(self, bytes)
    }
    fn shaped(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> &TextBlock {
        TextEngine::shaped(self, text, base, max_width, align)
    }
    fn shaped_run(
        &mut self,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
        scale: f32,
    ) -> &CachedRun {
        TextEngine::shaped_run(self, text, base, max_width, align, scale)
    }
    fn layout(
        &mut self,
        text: &str,
        base: TextStyle,
        ranges: &[(std::ops::Range<usize>, TextStyle)],
        max_width: Option<f32>,
        align: TextAlign,
    ) -> TextBlock {
        TextEngine::layout(self, text, base, ranges, max_width, align)
    }
}

impl TextBlockApi for TextBlock {
    fn width(&self) -> f32 {
        TextBlock::width(self)
    }
    fn height(&self) -> f32 {
        TextBlock::height(self)
    }
    fn size(&self) -> Size {
        TextBlock::size(self)
    }
    fn metrics(&self) -> TextMetrics {
        TextBlock::metrics(self)
    }
    fn missing_glyphs(&self) -> usize {
        TextBlock::missing_glyphs(self)
    }
    fn caret_pos(&self, byte: usize) -> (f32, f32, f32) {
        TextBlock::caret_pos(self, byte)
    }
    fn hit_to_byte(&self, x: f32, y: f32) -> usize {
        TextBlock::hit_to_byte(self, x, y)
    }
    fn selection_rects(&self, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)> {
        TextBlock::selection_rects(self, a, b)
    }
    fn render(&self, width: u32, height: u32, background: Color) -> RgbaImage {
        TextBlock::render(self, width, height, background)
    }
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
    shape_cache: HashMap<ShapeKey, Aged<TextBlock>>,
    /// Cache of origin-relative glyph runs keyed by `(ShapeKey, scale)` (R5). The
    /// paint layer translates + interns these instead of re-building the run each
    /// frame — the dominant display-list-emission cost for text.
    run_cache: HashMap<(ShapeKey, u32), Aged<CachedRun>>,
    /// Frame counter, advanced by [`TextEngine::begin_frame`]. Entries record the
    /// epoch they were last used in so [`sweep`] can tell the live working set
    /// from strings that merely passed through.
    epoch: u64,
    /// Current soft caps. Start at the base and retarget on each sweep.
    shape_cap: usize,
    run_cap: usize,
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
        // T.4: register the symbols fallback and append it to the script
        // fallback chains so chars the main face lacks (chevrons, arrows,
        // stars) shape through it instead of `.notdef`. `system_fonts:
        // false` means the chains start empty — without this, fallback
        // never engages and symbols render as tofu.
        let sym_blob = parley::fontique::Blob::new(std::sync::Arc::new(SYMBOLS_FONT));
        let sym = collection.register_fonts(sym_blob, None);
        if let Some((sym_id, _)) = sym.first() {
            use parley::fontique::Script;
            for script in [
                Script::COMMON, // symbols/punctuation runs resolve as Zyyy
                Script::from_bytes(*b"Latn"),
            ] {
                collection.append_fallbacks(
                    parley::fontique::FallbackKey::new(script, None),
                    std::iter::once(*sym_id),
                );
            }
        }
        TextEngine {
            font_cx: FontContext {
                collection,
                source_cache: parley::fontique::SourceCache::default(),
            },
            layout_cx: LayoutContext::new(),
            family,
            shape_cache: HashMap::default(),
            run_cache: HashMap::default(),
            epoch: 0,
            shape_cap: SHAPE_CACHE_CAP,
            run_cap: RUN_CACHE_CAP,
        }
    }

    /// Mark the start of a frame, so the shape/run caches can tell the live
    /// working set from strings that merely passed through.
    ///
    /// Call once per frame that actually shapes text. Skipping it is safe — the
    /// caches then fall back to the hard ceilings — but calling it on *idle*
    /// frames is not: advancing the epoch through a stretch where nothing is
    /// shaped would make the whole live set look stale, and the next sweep would
    /// discard it. `Headless::pump` calls this only on frames that build or
    /// restyle, which is the rule to copy for any other host.
    pub fn begin_frame(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
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
        self.shaped_by_key(key, text, base, max_width, align)
    }

    /// [`shaped`](Self::shaped) with the cache key already computed.
    ///
    /// R5: `shaped_run` needs the same key for its own `(ShapeKey, scale)`
    /// lookup, and used to build a second one by calling `shaped`, hashing the
    /// text twice per painted node per frame. `ShapeKey::new` was 2.2% of a
    /// 3000-row frame with two of the three constructions redundant.
    fn shaped_by_key(
        &mut self,
        key: ShapeKey,
        text: &str,
        base: &TextStyle,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> &TextBlock {
        if let Some(entry) = self.shape_cache.get_mut(&key) {
            entry.epoch = self.epoch;
        } else {
            let block = self.layout(text, base.clone(), &[], max_width, align);
            if self.shape_cache.len() >= self.shape_cap {
                sweep(
                    &mut self.shape_cache,
                    self.epoch,
                    &mut self.shape_cap,
                    SHAPE_CACHE_CAP,
                    SHAPE_CACHE_HARD_CAP,
                );
            }
            let epoch = self.epoch;
            self.shape_cache.insert(
                key,
                Aged {
                    value: block,
                    epoch,
                },
            );
        }
        &self.shape_cache[&key].value
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
        let shape_key = ShapeKey::new(text, base, max_width, align);
        let key = (shape_key, scale.to_bits());
        if let Some(entry) = self.run_cache.get_mut(&key) {
            entry.epoch = self.epoch;
        } else {
            let cached = {
                // R5: reuse the key just built rather than hashing the text again.
                let block = self.shaped_by_key(shape_key, text, base, max_width, align);
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
            if self.run_cache.len() >= self.run_cap {
                sweep(
                    &mut self.run_cache,
                    self.epoch,
                    &mut self.run_cap,
                    RUN_CACHE_CAP,
                    RUN_CACHE_HARD_CAP,
                );
            }
            let epoch = self.epoch;
            self.run_cache.insert(
                key,
                Aged {
                    value: cached,
                    epoch,
                },
            );
        }
        &self.run_cache[&key].value
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
        // PROP1: with a single upright face registered, fontique cannot match an
        // italic and reports a `skew` synthesis instead — handled beside the
        // existing faux-bold in the rasterizer.
        if let Some(v) = &base.variations {
            builder.push_default(StyleProperty::FontVariations(
                parley::FontVariations::Source(std::borrow::Cow::Borrowed(v.as_str())),
            ));
        }
        if let Some(f) = &base.features {
            builder.push_default(StyleProperty::FontFeatures(parley::FontFeatures::Source(
                std::borrow::Cow::Borrowed(f.as_str()),
            )));
        }
        builder.push_default(StyleProperty::FontStyle(if base.italic {
            parley::FontStyle::Italic
        } else {
            parley::FontStyle::Normal
        }));
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

    /// T.4 tofu detection: how many glyphs in this block mapped to `.notdef`
    /// (glyph id 0) — characters no registered face covers. The audit lint
    /// (`W0401`) reports blocks where this is non-zero; the lean (Latin
    /// subset) build makes uncovered scripts show up here instead of
    /// silently rendering boxes.
    pub fn missing_glyphs(&self) -> usize {
        let mut n = 0;
        for line in self.layout.lines() {
            for item in line.items() {
                let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                for glyph in glyph_run.positioned_glyphs() {
                    if glyph.id == 0 {
                        n += 1;
                    }
                }
            }
        }
        n
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
    /// M.6 (vectorial text): every glyph's outline as a filled Bézier path
    /// in logical px, positioned in layout space (y-down). Canvas consumers
    /// scale, stroke, or animate them freely — text as geometry, not
    /// bitmaps.
    pub fn outlines(&self) -> Vec<kurbo::BezPath> {
        let mut ctx = ScaleContext::new();
        let mut out = Vec::new();
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font = run.font();
                let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize)
                else {
                    continue;
                };
                let mut scaler = ctx
                    .builder(font_ref)
                    .size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .build();
                for glyph in glyph_run.positioned_glyphs() {
                    let Some(outline) = scaler.scale_outline(glyph.id as u16) else {
                        continue;
                    };
                    let (gx, gy) = (f64::from(glyph.x), f64::from(glyph.y));
                    // swash outlines are y-up; layout space is y-down.
                    let pt = |v: swash::zeno::Vector| {
                        kurbo::Point::new(gx + f64::from(v.x), gy - f64::from(v.y))
                    };
                    let mut path = kurbo::BezPath::new();
                    let points = outline.points();
                    let mut pi = 0usize;
                    for v in outline.verbs() {
                        match v {
                            swash::zeno::Verb::MoveTo => {
                                path.move_to(pt(points[pi]));
                                pi += 1;
                            }
                            swash::zeno::Verb::LineTo => {
                                path.line_to(pt(points[pi]));
                                pi += 1;
                            }
                            swash::zeno::Verb::QuadTo => {
                                path.quad_to(pt(points[pi]), pt(points[pi + 1]));
                                pi += 2;
                            }
                            swash::zeno::Verb::CurveTo => {
                                path.curve_to(
                                    pt(points[pi]),
                                    pt(points[pi + 1]),
                                    pt(points[pi + 2]),
                                );
                                pi += 3;
                            }
                            swash::zeno::Verb::Close => path.close_path(),
                        }
                    }
                    if !path.elements().is_empty() {
                        out.push(path);
                    }
                }
            }
        }
        out
    }

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
                // PROP1 faux italic, exactly parallel to the faux bold above:
                // with one upright face registered fontique cannot match an
                // italic, so it reports a skew (in degrees) for the rasterizer
                // to apply. zeno's skew is in the opposite sense, hence the
                // negation — a positive fontique skew leans the glyph right.
                let skew = run.synthesis().skew().unwrap_or(0.0);
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
                    skew_bits: skew.to_bits(),
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
                            if skew != 0.0 {
                                render.transform(Some(swash::zeno::Transform::skew(
                                    swash::zeno::Angle::from_degrees(-skew),
                                    swash::zeno::Angle::ZERO,
                                )));
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
            features: None,
            variations: None,
            italic: false,
            align: Default::default(),
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

    /// **The regression this policy exists for.** A live working set above
    /// `SHAPE_CACHE_CAP / 2`, plus one changing string per frame, used to lock
    /// the cache into permanent thrash: the sweep dropped to `cap / 2`, the same
    /// frame re-shaped what it still needed, and the cache re-crossed the cap
    /// immediately — measured at 1183 re-shapes/frame and a 2.2x frame-time
    /// penalty on a 2000-row list. Every live entry must survive.
    #[test]
    fn live_working_set_survives_a_cap_crossing() {
        let mut engine = TextEngine::new();
        let style = TextStyle::default();
        // Above cap/2 — this is the lock-in condition, not merely the trigger.
        let live = SHAPE_CACHE_CAP * 3 / 4;
        // Long enough that cumulative distinct strings crosses the cap (and then
        // keeps going), which is what used to trip the old policy.
        for frame in 0..(SHAPE_CACHE_CAP + live) {
            engine.begin_frame();
            engine.shaped(
                &format!("transient {frame}"),
                &style,
                None,
                TextAlign::Start,
            );
            for i in 0..live {
                engine.shaped(&format!("live {i}"), &style, None, TextAlign::Start);
            }
        }
        for i in 0..live {
            let key = ShapeKey::new(&format!("live {i}"), &style, None, TextAlign::Start);
            assert!(
                engine.shape_cache.contains_key(&key),
                "live entry {i} was evicted — the working set is thrashing"
            );
        }
        assert!(
            engine.shape_cache.len() <= SHAPE_CACHE_HARD_CAP,
            "cache grew past its ceiling: {}",
            engine.shape_cache.len()
        );
    }

    /// The other half of the contract: strings that merely pass through must
    /// still be reclaimed, or the cap would mean nothing. With no live set at
    /// all, the cache must stay bounded rather than growing to the hard cap.
    #[test]
    fn transient_strings_are_reclaimed() {
        let mut engine = TextEngine::new();
        let style = TextStyle::default();
        for frame in 0..(SHAPE_CACHE_CAP + SHAPE_CACHE_CAP / 2) {
            engine.begin_frame();
            engine.shaped(&format!("t{frame}"), &style, None, TextAlign::Start);
        }
        assert!(
            engine.shape_cache.len() <= SHAPE_CACHE_CAP,
            "transient strings accumulated: {}",
            engine.shape_cache.len()
        );
    }

    /// `sweep` in isolation, so the two branches are covered without paying for
    /// tens of thousands of real shaping calls.
    #[test]
    fn sweep_drops_stale_and_keeps_the_last_two_epochs() {
        let mut map: HashMap<u32, Aged<u32>> = HashMap::default();
        map.insert(0, Aged { value: 0, epoch: 1 }); // stale
        map.insert(1, Aged { value: 1, epoch: 9 }); // last frame
        map.insert(
            2,
            Aged {
                value: 2,
                epoch: 10,
            },
        ); // this frame
        let mut cap = 2;
        sweep(&mut map, 10, &mut cap, 2, 100);
        assert!(!map.contains_key(&0), "stale entry retained");
        assert!(map.contains_key(&1), "previous frame dropped mid-frame");
        assert!(map.contains_key(&2), "current frame dropped");
    }

    #[test]
    fn sweep_falls_back_to_halving_past_the_hard_cap() {
        let mut map: HashMap<u32, Aged<u32>> = HashMap::default();
        for i in 0..100u32 {
            map.insert(i, Aged { value: i, epoch: 5 });
        }
        let mut cap = 10;
        // Everything is live, so the epoch pass frees nothing and the ceiling
        // has to do the work.
        sweep(&mut map, 5, &mut cap, 10, 50);
        assert_eq!(map.len(), 50);
        assert_eq!(cap, 50, "cap must clamp to the hard ceiling");
    }
}
