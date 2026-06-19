//! `just win markdown` (or `just win-watch markdown examples/markdown/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    markdown::main_app().run(Size::new(560.0, 560.0));
}
