//! `just win websocket` (or `just win-watch websocket examples/websocket/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    websocket::main_app().run(Size::new(520.0, 540.0));
}
