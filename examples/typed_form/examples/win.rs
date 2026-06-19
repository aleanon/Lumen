//! `just run typed_form` (or `just run-hot typed_form` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    typed_form::main_app().run(Size::new(460.0, 560.0));
}
