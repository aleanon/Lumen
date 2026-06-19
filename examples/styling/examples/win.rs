//! `just win styling` (or `just win-watch styling examples/styling/app.lss` for
//! live theme reload). Opens the showcase in a real window.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    styling::main_app().run(Size::new(760.0, 760.0));
}
