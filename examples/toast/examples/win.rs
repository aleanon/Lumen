//! `just run toast` (or `just run-hot toast` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    toast::main_app().run(Size::new(520.0, 460.0));
}
