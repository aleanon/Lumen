//! `just run system_information` (or `just run-hot system_information` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    system_information::main_app().run(Size::new(520.0, 460.0));
}
