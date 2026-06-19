//! `just run gradient` (or `just run-hot gradient` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    gradient::main_app().run(Size::new(520.0, 480.0));
}
