//! `just win tour` (or `just win-watch tour examples/tour/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    tour::main_app().run(Size::new(540.0, 460.0));
}
