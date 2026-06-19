//! `just run counter` (or `just run-hot counter` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    counter::main_app().run(Size::new(440.0, 520.0));
}
