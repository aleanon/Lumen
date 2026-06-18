//! A1: the runtime is generic over the renderer — the CPU reference renderer is
//! the default, and a different backend can be swapped in at runtime.

use lumen_core::geometry::Size;
use lumen_core::Color;
use lumen_render::{cpu, DisplayList, Renderer, RgbaImage};
use lumen_widgets::{theme, App, BuildCx};

/// A stand-in alternate backend that ignores the scene and paints solid red —
/// enough to prove the runtime routes through the pluggable `Renderer`.
struct SolidRed;
impl Renderer for SolidRed {
    fn render_frame(&mut self, _l: &DisplayList, w: u32, h: u32, _s: f64, _bg: Color) -> RgbaImage {
        cpu::render(&DisplayList::new(), w, h, Color::srgb8(255, 0, 0, 255))
    }
    fn name(&self) -> &'static str {
        "solid-red"
    }
}

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width() + x) * 4) as usize;
    let p = img.pixels();
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

#[test]
fn renderer_is_pluggable() {
    let mut a = App::new(|_cx: &mut BuildCx| theme::center_screen(theme::display("hi")))
        .run_headless(Size::new(80.0, 60.0));
    a.pump();
    assert_eq!(a.renderer_name(), "cpu", "CPU is the default backend");
    let cpu_frame = a.screenshot();

    a.set_renderer(Box::new(SolidRed));
    assert_eq!(a.renderer_name(), "solid-red");
    let red_frame = a.screenshot();

    assert_ne!(
        cpu_frame, red_frame,
        "swapping the backend changes the output"
    );
    assert_eq!(
        px(&red_frame, 10, 10),
        [255, 0, 0, 255],
        "alt backend painted"
    );
}
