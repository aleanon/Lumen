//! `just run qr`.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    qr::main_app().run(Size::new(420.0, 380.0));
}
