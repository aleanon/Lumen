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
