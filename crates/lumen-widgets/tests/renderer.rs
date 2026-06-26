//! A1: the runtime is generic over the renderer — the CPU reference renderer is
//! the default, an alternate backend is chosen at construction (`with_renderer`),
//! and runtime swapping is retained for the `Box<dyn Renderer>` opt-in.

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
fn renderer_is_pluggable_at_construction() {
    let build = |_cx: &mut BuildCx| theme::center_screen(theme::display("hi"));

    let mut cpu_app = App::new(build).run_headless(Size::new(80.0, 60.0));
    cpu_app.pump();
    assert_eq!(cpu_app.renderer_name(), "cpu", "CPU is the default backend");
    let cpu_frame = cpu_app.screenshot();

    // Alternate backend selected at construction: App<SolidRed>.
    let mut red_app = App::new(build)
        .with_renderer(SolidRed)
        .run_headless(Size::new(80.0, 60.0));
    red_app.pump();
    assert_eq!(red_app.renderer_name(), "solid-red");
    let red_frame = red_app.screenshot();

    assert_ne!(cpu_frame, red_frame, "the backend changes the output");
    assert_eq!(
        px(&red_frame, 10, 10),
        [255, 0, 0, 255],
        "alt backend painted"
    );
}

#[test]
fn boxed_backend_is_swappable_at_runtime() {
    // The `Box<dyn Renderer>` opt-in keeps the type stable, so a same-type swap
    // is still legal — runtime backend selection for consumers who want it.
    let boxed: Box<dyn Renderer> = Box::new(SolidRed);
    let mut a = App::new(|_cx: &mut BuildCx| theme::display("x"))
        .with_renderer(boxed)
        .run_headless(Size::new(60.0, 40.0));
    a.pump();
    assert_eq!(a.renderer_name(), "solid-red");

    a.set_renderer(Box::new(lumen_render::TinySkia)); // same type: Box<dyn Renderer>
    assert_eq!(a.renderer_name(), "cpu", "swapped to CPU at runtime");
}
