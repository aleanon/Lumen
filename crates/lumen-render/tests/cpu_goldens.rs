//! T0.4 acceptance: golden PNGs per command class (exact compare on the CPU
//! reference renderer), bit-determinism, and damage-region equivalence.
//!
//! Goldens live at `tests/golden/cpu/<name>.png`. Re-record with
//! `LUMEN_UPDATE_GOLDENS=1 cargo test -p lumen-render`. CI never sets it.

use kurbo::{BezPath, Point, Rect};
use lumen_core::Color;
use lumen_render::cpu;
use lumen_render::display_list::*;
use lumen_render::RgbaImage;
use std::path::PathBuf;

const W: u32 = 80;
const H: u32 = 60;

fn bg() -> Color {
    Color::srgb8(255, 255, 255, 255)
}

fn golden_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"))
}

/// Exact golden compare (CPU renderer is deterministic by contract, 02 §7).
fn check_golden(name: &str, img: &RgbaImage) {
    let path = golden_file(name);
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("missing golden {path:?}; run with LUMEN_UPDATE_GOLDENS=1"));
    let expected = RgbaImage::from_png(&bytes).unwrap();
    if img != &expected {
        let actual = path.with_extension("actual.png");
        std::fs::write(&actual, img.to_png()).unwrap();
        panic!(
            "golden mismatch for {name}: {} pixels differ; wrote {actual:?}",
            img.diff_count(&expected)
        );
    }
}

fn render(list: &DisplayList) -> RgbaImage {
    cpu::render(list, W, H, bg())
}

// --- scenes -----------------------------------------------------------------

fn scene_rect() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(10.0, 8.0, 70.0, 52.0),
        brush: Brush::Solid(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
        radii: CornerRadii::all(10.0),
        border: Some(Border {
            width: 3.0,
            color: Color::srgb8(0x0b, 0x3d, 0x91, 0xff),
        }),
    });
    dl
}

fn scene_path() -> DisplayList {
    let mut dl = DisplayList::new();
    let mut tri = BezPath::new();
    tri.move_to((40.0, 6.0));
    tri.line_to((72.0, 54.0));
    tri.line_to((8.0, 54.0));
    tri.close_path();
    dl.push(DrawCmd::Path {
        path: tri.clone(),
        brush: Brush::Solid(Color::srgb8(0x2e, 0xa0, 0x43, 0xff)),
        style: FillOrStroke::Fill,
    });
    let mut wave = BezPath::new();
    wave.move_to((8.0, 30.0));
    wave.quad_to((28.0, 8.0), (40.0, 30.0));
    wave.quad_to((52.0, 52.0), (72.0, 30.0));
    dl.push(DrawCmd::Path {
        path: wave,
        brush: Brush::Solid(Color::srgb8(0x80, 0x10, 0x10, 0xff)),
        style: FillOrStroke::Stroke { width: 4.0 },
    });
    dl
}

fn ramp() -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: Color::srgb8(0xff, 0x00, 0x00, 0xff),
        },
        GradientStop {
            offset: 0.5,
            color: Color::srgb8(0x00, 0xff, 0x00, 0xff),
        },
        GradientStop {
            offset: 1.0,
            color: Color::srgb8(0x00, 0x00, 0xff, 0xff),
        },
    ]
}

fn scene_with_brush(brush: Brush) -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Rect {
        rect: Rect::new(6.0, 6.0, 74.0, 54.0),
        brush,
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl
}

fn scene_linear() -> DisplayList {
    scene_with_brush(Brush::LinearGradient {
        start: Point::new(6.0, 6.0),
        end: Point::new(74.0, 6.0),
        stops: ramp(),
        spread: SpreadMode::Pad,
    })
}

fn scene_radial() -> DisplayList {
    scene_with_brush(Brush::RadialGradient {
        center: Point::new(40.0, 30.0),
        radius: 34.0,
        stops: ramp(),
        spread: SpreadMode::Pad,
    })
}

fn scene_conic() -> DisplayList {
    scene_with_brush(Brush::ConicGradient {
        center: Point::new(40.0, 30.0),
        start_angle: 0.0,
        stops: ramp(),
    })
}

fn scene_image() -> DisplayList {
    // 2x2 checkerboard, scaled up with nearest sampling.
    let r = Color::srgb8(220, 40, 40, 255).to_srgb8();
    let y = Color::srgb8(250, 210, 60, 255).to_srgb8();
    let mut px = Vec::new();
    for (a, b) in [(r, y), (y, r)] {
        px.extend_from_slice(&a);
        px.extend_from_slice(&b);
    }
    let img = RgbaImage::from_raw(2, 2, px);
    let mut dl = DisplayList::new();
    dl.images.push(img);
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: Rect::new(0.0, 0.0, 2.0, 2.0),
        dst_rect: Rect::new(8.0, 8.0, 64.0, 44.0),
        quality: Filter::Nearest,
    });
    dl
}

fn scene_layer() -> DisplayList {
    let mut dl = DisplayList::new();
    // background fill
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, 80.0, 60.0),
        brush: Brush::Solid(Color::srgb8(0xee, 0xee, 0xee, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::PushLayer {
        clip: Some(RoundedRect {
            rect: Rect::new(16.0, 12.0, 64.0, 48.0),
            radii: CornerRadii::all(12.0),
        }),
        opacity: 0.6,
        transform: kurbo::Affine::IDENTITY,
        blend: BlendMode::SourceOver,
    });
    dl.push(DrawCmd::Rect {
        rect: Rect::new(0.0, 0.0, 80.0, 60.0),
        brush: Brush::Solid(Color::srgb8(0xe8, 0x1a, 0x4b, 0xff)),
        radii: CornerRadii::ZERO,
        border: None,
    });
    dl.push(DrawCmd::PopLayer);
    dl
}

fn scene_shader() -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::Shader {
        id: ShaderId(0),
        rect: Rect::new(12.0, 10.0, 68.0, 50.0),
        uniforms: UniformBlock {
            fallback: Color::srgb8(0x66, 0x33, 0x99, 0xff),
        },
    });
    dl
}

// --- golden tests (one per command class) -----------------------------------

#[test]
fn golden_rect() {
    check_golden("rect", &render(&scene_rect()));
}
#[test]
fn golden_path() {
    check_golden("path", &render(&scene_path()));
}
#[test]
fn golden_gradient_linear() {
    check_golden("gradient_linear", &render(&scene_linear()));
}
#[test]
fn golden_gradient_radial() {
    check_golden("gradient_radial", &render(&scene_radial()));
}
#[test]
fn golden_gradient_conic() {
    check_golden("gradient_conic", &render(&scene_conic()));
}
#[test]
fn golden_image() {
    check_golden("image", &render(&scene_image()));
}
#[test]
fn golden_layer() {
    check_golden("layer", &render(&scene_layer()));
}
#[test]
fn golden_shader_fallback() {
    check_golden("shader_fallback", &render(&scene_shader()));
}

// --- determinism + damage ---------------------------------------------------

#[test]
fn same_scene_renders_byte_identical() {
    let dl = scene_rect();
    assert_eq!(
        render(&dl),
        render(&dl),
        "CPU renderer must be deterministic"
    );
    // also across all classes
    for dl in [scene_path(), scene_linear(), scene_conic(), scene_layer()] {
        assert_eq!(render(&dl), render(&dl));
    }
}

#[test]
fn damage_region_equals_full_render_cropped() {
    let dl = scene_path();
    let full = render(&dl);
    // integer-aligned dirty rect
    let dirty = Rect::new(20.0, 16.0, 60.0, 48.0);
    // The damage render returns a dirty-sized image; it must equal the full
    // render cropped to the same rect, byte-for-byte.
    let damaged = cpu::render_damage(&dl, W, H, bg(), dirty);
    let fc = full.crop(20, 16, 40, 32);
    assert_eq!(damaged.width(), 40);
    assert_eq!(damaged.height(), 32);
    assert_eq!(
        damaged.diff_count(&fc),
        0,
        "dirty-rect re-render must equal full render cropped"
    );
}
