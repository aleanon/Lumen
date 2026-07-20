//! `just run exit`.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    exit::main_app().run(Size::new(420.0, 320.0));
}
