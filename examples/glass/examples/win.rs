//! `just run glass` (or `just run-hot glass` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    glass::main_app().run(Size::new(600.0, 520.0));
}
