//! `just win todos` (or `just win-watch todos examples/todos/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    todos::main_app().run(Size::new(520.0, 520.0));
}
