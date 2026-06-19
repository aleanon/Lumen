//! `just run clock` (or `just run-hot clock` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    clock::main_app().run(Size::new(460.0, 500.0));
}
