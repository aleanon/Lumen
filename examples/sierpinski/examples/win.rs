//! `just win sierpinski` (or `just win-watch sierpinski examples/sierpinski/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    sierpinski::main_app().run(Size::new(460.0, 560.0));
}
