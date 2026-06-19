//! `just win svg` (or `just win-watch svg examples/svg/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    svg::main_app().run(Size::new(520.0, 420.0));
}
