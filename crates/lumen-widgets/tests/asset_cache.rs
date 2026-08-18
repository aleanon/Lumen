//! CACHE1: the decoded-image cache must not grow without bound.
//!
//! It had no cap and no `clear()`, so every distinct image an app ever decoded
//! stayed resident for the process lifetime — as decoded RGBA, not compressed
//! source. An app cycling through images (a gallery, avatars, anything
//! data-driven) grew until it died.

use lumen_widgets::asset;

/// A tiny valid PNG whose pixel content varies with `seed`, so each call
/// produces a distinct cache key.
fn png(seed: u8) -> Vec<u8> {
    let img = lumen_widgets::RgbaImage::from_raw(1, 1, vec![seed, seed, seed, 255]);
    img.to_png()
}

#[test]
fn cache_stays_bounded_under_many_distinct_images() {
    asset::clear_cache();
    for seed in 0..200u8 {
        asset::png(&png(seed)).expect("decodes");
    }
    let len = asset::cache_len();
    assert!(
        len <= 64,
        "decode cache must stay bounded; retained {len} images"
    );
}

#[test]
fn clearing_releases_everything() {
    // The hook memory-pressure handlers need: the cache is pure derived data,
    // so releasing it costs a re-decode and nothing else.
    asset::clear_cache();
    asset::png(&png(1)).expect("decodes");
    assert!(asset::cache_len() > 0);
    asset::clear_cache();
    assert_eq!(asset::cache_len(), 0);
}

#[test]
fn caching_still_works_for_repeats() {
    // The bound must not defeat the cache's purpose: decoding the same bytes
    // twice should still hit.
    asset::clear_cache();
    let bytes = png(7);
    asset::png(&bytes).expect("decodes");
    let after_first = asset::cache_len();
    asset::png(&bytes).expect("decodes");
    assert_eq!(
        asset::cache_len(),
        after_first,
        "a repeat decode must hit the cache, not add an entry"
    );
}

// ---------------------------------------------------------------------------
// The two holes CACHE1 left behind. Both were found by an independent
// coverage audit, and both are real defects rather than merely untested code:
// each test below FAILS against the pre-fix implementation.
// ---------------------------------------------------------------------------

/// `png()` capped the cache; `decode()` — the jpeg/gif/webp entry point that
/// `image_any` routes through — did not, so CACHE1's unbounded growth was
/// still live on every format except PNG.
#[cfg(feature = "codecs")]
#[test]
fn decode_honours_the_cap_that_png_does() {
    asset::clear_cache();
    for seed in 0..200u8 {
        asset::decode(&png(seed)).expect("decodes");
    }
    let len = asset::cache_len();
    assert!(
        len <= 64,
        "decode() shares CACHE with png() and must share its bound; \
         retained {len} images"
    );
}

/// `clear_cache()` is documented as "Drop every decoded image on this thread"
/// and is what the iOS memory-warning and Android LowMemory handlers call. It
/// cleared the still-image cache only, leaving the decoded animation frames —
/// the largest entries in the process — resident.
#[cfg(feature = "codecs")]
#[test]
fn clearing_releases_animations_not_just_still_images() {
    const GIF: &[u8] = include_bytes!("assets/anim.gif");
    asset::clear_cache();
    asset::animation(GIF).expect("gif decodes");
    assert!(
        asset::anim_cache_len() > 0,
        "precondition: the animation should be cached after decoding"
    );

    asset::clear_cache();
    assert_eq!(
        asset::anim_cache_len(),
        0,
        "a memory-pressure clear must release decoded animation frames — they \
         are the biggest thing it exists to free"
    );
}

/// …and the animation cache itself must be bounded. Distinct keys are made by
/// appending bytes after the GIF trailer, which decoders ignore.
#[cfg(feature = "codecs")]
#[test]
fn animation_cache_stays_bounded() {
    const GIF: &[u8] = include_bytes!("assets/anim.gif");
    asset::clear_cache();
    for i in 0..40u8 {
        let mut bytes = GIF.to_vec();
        bytes.push(i); // past the 0x3B trailer: new content hash, same image
        asset::animation(&bytes).expect("gif decodes");
    }
    let len = asset::anim_cache_len();
    assert!(
        len <= 8,
        "the animation cache must stay bounded; retained {len} animations, \
         each of which is N full RGBA frames"
    );
}
