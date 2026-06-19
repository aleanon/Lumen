//! `just run websocket` (or `just run-hot websocket` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    websocket::main_app().run(Size::new(520.0, 540.0));
}
