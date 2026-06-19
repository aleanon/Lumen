//! `just run events` (or `just run-hot events` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    events::main_app().run(Size::new(460.0, 440.0));
}
