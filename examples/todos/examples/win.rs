//! `just run todos` (or `just run-hot todos` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    todos::main_app().run(Size::new(520.0, 520.0));
}
