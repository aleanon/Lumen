//! `just win changelog` (or `just win-watch changelog examples/changelog/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    changelog::main_app().run(Size::new(560.0, 560.0));
}
