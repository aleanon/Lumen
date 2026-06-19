//! `just run progress_bar` (or `just run-hot progress_bar` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    progress_bar::main_app().run(Size::new(480.0, 500.0));
}
