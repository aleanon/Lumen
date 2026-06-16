//! T6.2: minimal SVG renders to deterministic pixels (golden + shape checks).
use lumen_core::Color;
use lumen_render::{svg, RgbaImage};
use std::path::PathBuf;

const SVG: &str = r##"<svg width="40" height="40">
  <rect x="2" y="2" width="16" height="16" fill="#1a73e8"/>
  <circle cx="30" cy="10" r="8" fill="#2ea043"/>
  <path d="M 4 38 L 20 22 L 36 38 Z" fill="#e8404b"/>
</svg>"##;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"))
}

#[test]
fn svg_renders_shapes() {
    let img = svg::render(SVG, 40, 40, Color::WHITE);
    assert_eq!((img.width(), img.height()), (40, 40));

    // The blue rect region is blue-ish; a white corner stays white.
    let px = |x: u32, y: u32| {
        let i = ((y * 40 + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2]]
    };
    let [r, g, b] = px(8, 8);
    assert!(b > 150 && r < 120, "blue rect at (8,8): {r},{g},{b}");
    assert_eq!(px(0, 39), [255, 255, 255], "bottom-left corner white");

    // Golden (exact; CPU renderer is deterministic).
    let path = golden("svg_shapes");
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let expected = RgbaImage::from_png(
        &std::fs::read(&path)
            .unwrap_or_else(|_| panic!("missing {path:?}; run LUMEN_UPDATE_GOLDENS=1")),
    )
    .unwrap();
    assert_eq!(img.diff_count(&expected), 0, "svg golden mismatch");
}
