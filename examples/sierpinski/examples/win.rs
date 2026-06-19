//! `just run sierpinski` (or `just run-hot sierpinski` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    sierpinski::main_app().run(Size::new(460.0, 560.0));
}
