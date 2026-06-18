//! B3: PNG assets decode correctly and are cached by content.

use lumen_render::RgbaImage;
use lumen_widgets::asset;

#[test]
fn png_decodes_and_caches() {
    // Build a known image, encode to PNG, then decode through the asset cache.
    let src = RgbaImage::new(8, 5);
    let bytes = src.to_png();

    assert!(!asset::is_cached(&bytes), "cold");
    let a = asset::png(&bytes).expect("decode");
    assert_eq!((a.width(), a.height()), (8, 5));
    assert!(asset::is_cached(&bytes), "warm after first decode");

    // Second call hits the cache and yields an identical image.
    let b = asset::png(&bytes).expect("decode");
    assert_eq!(a, b);

    // A bad PNG is an error, not a panic.
    assert!(asset::png(b"not a png").is_err());
    // ...and the Element helper degrades to a placeholder instead of panicking.
    let _ = asset::image_png(b"not a png");
}
