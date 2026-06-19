//! `just run svg` (or `just run-hot svg` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    svg::main_app().run(Size::new(520.0, 420.0));
}
