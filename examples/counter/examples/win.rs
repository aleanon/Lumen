//! `just win counter` (or `just win-watch counter examples/counter/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    counter::main_app().run(Size::new(440.0, 520.0));
}
