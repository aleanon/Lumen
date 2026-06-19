//! `just win progress_bar` (or `just win-watch progress_bar examples/progress_bar/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    progress_bar::main_app().run(Size::new(480.0, 500.0));
}
