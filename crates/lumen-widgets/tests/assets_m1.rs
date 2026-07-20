//! M.1 (ADR-M1): jpeg/gif/webp decode through the shared content-keyed cache,
//! and the animated-image asset type on the virtual clock.

use lumen_widgets::asset;

const JPG: &[u8] = include_bytes!("assets/red.jpg");
const WEBP: &[u8] = include_bytes!("assets/green.webp");
const GIF: &[u8] = include_bytes!("assets/anim.gif");
const PNG: &[u8] = include_bytes!("assets/dot.png");

#[test]
fn decode_covers_png_jpeg_webp_gif() {
    for (name, bytes) in [("png", PNG), ("jpg", JPG), ("webp", WEBP), ("gif", GIF)] {
        let img = asset::decode(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((img.width(), img.height()), (4, 4), "{name} dims");
    }
    // jpeg is lossy but a solid block stays in the right neighborhood.
    let img = asset::decode(JPG).unwrap();
    let px = img.pixels();
    assert!(
        px[0] > 150 && px[1] < 100 && px[2] < 100,
        "red-ish: {:?}",
        &px[..4]
    );
    // Second decode hits the shared cache.
    assert!(asset::is_cached(JPG));
}

#[test]
fn animation_frames_and_clock_schedule() {
    let a = asset::animation(GIF).expect("gif decodes");
    assert_eq!(a.frames.len(), 3);
    assert_eq!(a.delays_ms, vec![100.0, 100.0, 100.0]);
    assert_eq!(a.duration_ms(), 300.0);
    // Frame selection on a looping timeline + the next wake edge.
    assert_eq!(a.frame_at(0.0), (0, 100.0));
    assert_eq!(a.frame_at(150.0), (1, 200.0));
    assert_eq!(a.frame_at(299.0), (2, 300.0));
    assert_eq!(a.frame_at(310.0), (0, 400.0), "loops");
    // Distinct frame contents (R,G,B).
    assert!(a.frames[0].pixels()[0] > 200);
    assert!(a.frames[1].pixels()[1] > 200);
    assert!(a.frames[2].pixels()[2] > 200);
}

#[test]
fn animated_element_plays_on_the_virtual_clock() {
    use kurbo::Size;
    use lumen_widgets::{widgets, App};
    let mut h = App::new(|cx| widgets::column(vec![asset::animated(cx, GIF).id("gif")]))
        .run_headless(Size::new(100.0, 100.0));
    h.pump();
    let shot0 = h.screenshot();
    // Advance past the first frame edge: the next frame must render.
    h.advance_clock(150.0);
    h.pump();
    let shot1 = h.screenshot();
    assert_ne!(
        shot0.pixels(),
        shot1.pixels(),
        "frame advanced with the clock"
    );
    // And the third frame.
    h.advance_clock(100.0);
    h.pump();
    let shot2 = h.screenshot();
    assert_ne!(shot1.pixels(), shot2.pixels());
}
