//! B3 + M.1 — image assets: decode with a content-keyed cache so repeated
//! builds reuse the decode instead of re-decoding every frame, plus helpers
//! that drop decoded images straight into the tree. PNG rides the
//! dependency-free tiny-skia path (the deterministic core); jpeg/gif/webp
//! come from the `image` crate behind the default-on `codecs` feature
//! (ADR-M1: pure-Rust decoders only, no encoders, avif deferred — the lean
//! profile drops the whole stack). Animated GIFs decode to an [`Animation`]
//! and play on the virtual clock via [`animated`].

use crate::{widgets, Element};
use lumen_render::RgbaImage;
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

thread_local! {
    static CACHE: RefCell<HashMap<u64, RgbaImage>> = RefCell::new(HashMap::new());
}

fn key(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Decode PNG `bytes` into an [`RgbaImage`], caching by content hash so the
/// (expensive) decode happens once even if the same asset is requested every
/// frame. Errors surface as `Err` (a malformed PNG is data, not a panic).
pub fn png(bytes: &[u8]) -> Result<RgbaImage, String> {
    let k = key(bytes);
    if let Some(img) = CACHE.with(|c| c.borrow().get(&k).cloned()) {
        return Ok(img);
    }
    let img = RgbaImage::from_png(bytes)?;
    CACHE.with(|c| c.borrow_mut().insert(k, img.clone()));
    Ok(img)
}

/// True if `bytes` is already in the decode cache (test/diagnostic aid).
pub fn is_cached(bytes: &[u8]) -> bool {
    CACHE.with(|c| c.borrow().contains_key(&key(bytes)))
}

/// Decode PNG `bytes` (cached) into an image [`Element`]. A decode failure yields
/// a 1×1 transparent placeholder so a bad asset can't take down a build.
pub fn image_png(bytes: &[u8]) -> Element {
    let img = png(bytes).unwrap_or_else(|_| RgbaImage::new(1, 1));
    widgets::image(img)
}

// --- M.1 (ADR-M1): jpeg / gif / webp via the `image` crate -------------------

/// Sniff + decode any supported raster format (PNG always; jpeg/gif/webp with
/// the default-on `codecs` feature), through the same content-keyed cache.
/// For animated GIFs this yields the first frame — use [`animation`] for the
/// frame sequence.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    let k = key(bytes);
    if let Some(img) = CACHE.with(|c| c.borrow().get(&k).cloned()) {
        return Ok(img);
    }
    let img = decode_uncached(bytes)?;
    CACHE.with(|c| c.borrow_mut().insert(k, img.clone()));
    Ok(img)
}

fn decode_uncached(bytes: &[u8]) -> Result<RgbaImage, String> {
    // PNG stays on the dependency-free tiny-skia path (deterministic core).
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return RgbaImage::from_png(bytes);
    }
    #[cfg(feature = "codecs")]
    {
        let dyn_img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = dyn_img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        Ok(RgbaImage::from_raw(w, h, rgba.into_raw()))
    }
    #[cfg(not(feature = "codecs"))]
    Err("unsupported image format (lean build: PNG only — enable `codecs`)".into())
}

/// Decode any supported format (cached) into an image [`Element`]; failures
/// yield a 1×1 transparent placeholder (bad assets are data, not panics).
pub fn image_any(bytes: &[u8]) -> Element {
    let img = decode(bytes).unwrap_or_else(|_| RgbaImage::new(1, 1));
    widgets::image(img)
}

/// A decoded animation: frames plus per-frame delays (ms). Total duration is
/// the delay sum; frames render via [`animated`] on the virtual clock.
#[derive(Clone)]
pub struct Animation {
    /// Decoded frames.
    pub frames: Vec<RgbaImage>,
    /// Per-frame delay in ms (same length as `frames`; 100 ms floor for the
    /// zero-delay GIFs browsers also clamp).
    pub delays_ms: Vec<f64>,
}

impl Animation {
    /// Sum of all frame delays (ms).
    pub fn duration_ms(&self) -> f64 {
        self.delays_ms.iter().sum()
    }

    /// The frame index active at `t_ms` on a looping timeline, and the
    /// absolute time (ms) at which the NEXT frame is due (for `wake_at`).
    pub fn frame_at(&self, t_ms: f64) -> (usize, f64) {
        let total = self.duration_ms().max(1.0);
        let cycle_start = (t_ms / total).floor() * total;
        let mut acc = cycle_start;
        for (i, d) in self.delays_ms.iter().enumerate() {
            if t_ms < acc + d {
                return (i, acc + d);
            }
            acc += d;
        }
        (self.frames.len().saturating_sub(1), acc)
    }
}

#[cfg(feature = "codecs")]
thread_local! {
    static ANIM_CACHE: RefCell<HashMap<u64, Animation>> = RefCell::new(HashMap::new());
}

/// Decode an animated GIF into its frame sequence (cached by content hash).
/// Single-frame inputs yield a one-frame animation.
#[cfg(feature = "codecs")]
pub fn animation(bytes: &[u8]) -> Result<Animation, String> {
    use image::AnimationDecoder;
    let k = key(bytes);
    if let Some(a) = ANIM_CACHE.with(|c| c.borrow().get(&k).cloned()) {
        return Ok(a);
    }
    let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut delays = Vec::new();
    for f in dec.into_frames() {
        let f = f.map_err(|e| e.to_string())?;
        let (num, den) = f.delay().numer_denom_ms();
        delays.push(f64::from(num) / f64::from(den.max(1)));
        let buf = f.into_buffer();
        let (w, h) = (buf.width(), buf.height());
        frames.push(RgbaImage::from_raw(w, h, buf.into_raw()));
    }
    if frames.is_empty() {
        return Err("no frames".into());
    }
    // 0-delay frames get the 100 ms floor browsers apply.
    for d in &mut delays {
        if *d <= 0.0 {
            *d = 100.0;
        }
    }
    let a = Animation {
        frames,
        delays_ms: delays,
    };
    ANIM_CACHE.with(|c| c.borrow_mut().insert(k, a.clone()));
    Ok(a)
}

/// An animated-GIF [`Element`] playing on the app's virtual clock: renders
/// the frame due at `now_ms` and schedules a wake for the next frame edge.
/// Decode failures fall back to [`image_any`] (first frame or placeholder).
#[cfg(feature = "codecs")]
pub fn animated(cx: &crate::BuildCx, bytes: &[u8]) -> Element {
    match animation(bytes) {
        Ok(a) => {
            let (idx, next_ms) = a.frame_at(cx.now_ms());
            cx.wake_at(next_ms);
            widgets::image(a.frames[idx].clone())
        }
        Err(_) => image_any(bytes),
    }
}
