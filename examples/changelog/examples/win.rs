//! `just run changelog` (or `just run-hot changelog` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    changelog::main_app().run(Size::new(560.0, 560.0));
}
