//! `just run multi_window`.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    multi_window::main_app().run(Size::new(420.0, 320.0));
}
