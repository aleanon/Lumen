//! `just win events` (or `just win-watch events examples/events/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    events::main_app().run(Size::new(460.0, 440.0));
}
