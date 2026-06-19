//! Renders color_palette to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = color_palette::main_app().run_headless(Size::new(580.0, 480.0));
    let s = a.pump();
    std::fs::write("/tmp/color_palette.png", a.screenshot().to_png()).unwrap();
    println!(
        "color_palette: {} nodes -> /tmp/color_palette.png",
        s.node_count
    );
}
