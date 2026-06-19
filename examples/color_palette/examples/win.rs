//! `just win color_palette` (or `just win-watch color_palette examples/color_palette/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    color_palette::main_app().run(Size::new(580.0, 480.0));
}
