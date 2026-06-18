//! B3 — image assets: decode (PNG) with a content-keyed cache so repeated builds
//! reuse the decode instead of re-decoding every frame, plus a helper that drops
//! a decoded image straight into the tree. Additional codecs (jpeg/webp/avif)
//! are follow-on work (new deps → ADR-003); PNG rides the existing `RgbaImage`
//! path with no new dependency.

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
