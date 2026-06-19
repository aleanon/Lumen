//! `just win loading_spinners` (or `just win-watch loading_spinners examples/loading_spinners/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    loading_spinners::main_app().run(Size::new(520.0, 360.0));
}
