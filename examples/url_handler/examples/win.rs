//! `just run url_handler`.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    url_handler::main_app().run(Size::new(520.0, 320.0));
}
