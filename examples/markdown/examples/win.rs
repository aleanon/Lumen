//! `just run markdown` (or `just run-hot markdown` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    markdown::main_app().run(Size::new(560.0, 560.0));
}
