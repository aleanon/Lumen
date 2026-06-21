//! `just win chart` (or `just win-watch chart examples/chart/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    chart::main_app().run(Size::new(600.0, 640.0));
}
