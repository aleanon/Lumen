//! `just run chart` (or `just run-hot chart` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    chart::main_app().run(Size::new(600.0, 640.0));
}
