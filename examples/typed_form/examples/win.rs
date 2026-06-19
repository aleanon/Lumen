//! `just win typed_form` (or `just win-watch typed_form examples/typed_form/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    typed_form::main_app().run(Size::new(460.0, 560.0));
}
