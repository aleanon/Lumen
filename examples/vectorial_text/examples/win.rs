//! `just run vectorial_text`.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    vectorial_text::main_app().run(Size::new(560.0, 340.0));
}
