//! `just win image` (or `just win-watch image examples/image/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    image::main_app().run(Size::new(560.0, 540.0));
}
