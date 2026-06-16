//! E8.1: the Canvas widget draws shapes/paths/transforms to deterministic pixels.
use kurbo::{Affine, BezPath, Point, Size};
use lumen_core::Color;
use lumen_render::RgbaImage;
use lumen_widgets::{widgets, App};
use std::f64::consts::PI;
use std::path::PathBuf;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"))
}

#[test]
fn canvas_draws_a_clock_face() {
    let app = App::new(|_| {
        widgets::canvas(80.0, 80.0, |f, size: Size| {
            let c = Point::new(size.width / 2.0, size.height / 2.0);
            // Face.
            f.fill_circle(c, 36.0, Color::srgb8(0x1a, 0x73, 0xe8, 0xff));
            f.fill_circle(c, 32.0, Color::WHITE);
            // An hour hand rotated 90°, drawn as a stroked path about the centre.
            let mut hand = BezPath::new();
            hand.move_to(c);
            hand.line_to((c.x, c.y - 24.0));
            f.with_transform(
                Affine::translate((c.x, c.y))
                    * Affine::rotate(PI / 2.0)
                    * Affine::translate((-c.x, -c.y)),
                |f| f.stroke(&hand, Color::BLACK, 3.0),
            );
        })
    });
    let img: RgbaImage = app.run_headless(Size::new(80.0, 80.0)).screenshot();
    assert_eq!((img.width(), img.height()), (80, 80));

    // Spot-check: centre is white (inner face), a blue ring pixel exists.
    let px = |x: u32, y: u32| {
        let i = ((y * 80 + x) * 4) as usize;
        let p = img.pixels();
        [p[i], p[i + 1], p[i + 2]]
    };
    // Above-centre is white face (the hand points right after the 90° rotation).
    assert_eq!(px(40, 20), [255, 255, 255], "face white above centre");
    let blue = img
        .pixels()
        .chunks_exact(4)
        .any(|p| p[2] > 150 && p[0] < 120);
    assert!(blue, "blue face ring drawn");
    let dark_right = (44..62).any(|x| px(x, 40)[0] < 80 && px(x, 40)[2] < 80);
    assert!(dark_right, "hand drawn rightward");

    let path = golden("canvas_clock");
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let expected = RgbaImage::from_png(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(img.diff_count(&expected), 0, "canvas golden mismatch");
}
