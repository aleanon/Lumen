//! `just run data` (or `just run-hot data` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    data::main_app().run(Size::new(460.0, 460.0));
}
