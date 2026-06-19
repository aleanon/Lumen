//! `just win modal` (or `just win-watch modal examples/modal/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    modal::main_app().run(Size::new(540.0, 460.0));
}
