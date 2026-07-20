//! E.3: asset decode must never panic — PNG bytes and SVG sources arrive
//! from the filesystem/network as untrusted data.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lumen_render::RgbaImage::from_png(data);
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = lumen_render::svg::parse(s);
    }
});
