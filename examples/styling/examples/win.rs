//! `just run styling` (or `just run-hot styling` for live `.lss` reload). Opens the showcase in a real window.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    styling::main_app().run(Size::new(760.0, 760.0));
}
