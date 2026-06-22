//! `just run catalog` (or `just run-hot catalog` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    catalog::main_app().run(Size::new(520.0, 640.0));
}
