//! `just run modal` (or `just run-hot modal` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    modal::main_app().run(Size::new(540.0, 460.0));
}
