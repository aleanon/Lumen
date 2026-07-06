//! `just run accordion` (or `just run-agent accordion` for the live agent port).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    accordion::main_app().run(Size::new(520.0, 620.0));
}
