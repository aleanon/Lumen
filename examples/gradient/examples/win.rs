//! `just win gradient` (or `just win-watch gradient examples/gradient/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    gradient::main_app().run(Size::new(520.0, 480.0));
}
