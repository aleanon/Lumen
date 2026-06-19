//! `just run color_palette` (or `just run-hot color_palette` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    color_palette::main_app().run(Size::new(580.0, 480.0));
}
