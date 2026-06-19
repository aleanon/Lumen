//! `just run image` (or `just run-hot image` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    image::main_app().run(Size::new(560.0, 540.0));
}
