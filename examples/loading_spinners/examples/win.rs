//! `just run loading_spinners` (or `just run-hot loading_spinners` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    loading_spinners::main_app().run(Size::new(520.0, 360.0));
}
