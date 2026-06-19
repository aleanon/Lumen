//! `just win clock` (or `just win-watch clock examples/clock/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    clock::main_app().run(Size::new(460.0, 500.0));
}
