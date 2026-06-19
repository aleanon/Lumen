//! `just win system_information` (or `just win-watch system_information examples/system_information/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    system_information::main_app().run(Size::new(520.0, 460.0));
}
