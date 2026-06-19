//! `just run tour` (or `just run-hot tour` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    tour::main_app().run(Size::new(540.0, 460.0));
}
