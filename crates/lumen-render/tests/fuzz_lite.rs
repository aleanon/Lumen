//! E.3 fuzz-lite: PNG and SVG decode never panic on arbitrary bytes (bounded,
//! every gate; the libFuzzer `decode` target goes deeper nightly).
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn png_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = lumen_render::RgbaImage::from_png(&bytes);
    }

    #[test]
    fn png_decode_survives_corrupted_headers(tail in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend(tail);
        let _ = lumen_render::RgbaImage::from_png(&bytes);
    }

    #[test]
    fn svg_parse_never_panics(src in ".{0,400}") {
        let _ = lumen_render::svg::parse(&src);
    }

    #[test]
    fn svg_parse_survives_tag_soup(
        tags in proptest::collection::vec("[a-zA-Z/<>= \"#.0-9-]{0,40}", 0..8),
    ) {
        let src = format!("<svg>{}</svg>", tags.join("<"));
        let _ = lumen_render::svg::parse(&src);
    }
}
