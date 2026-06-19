//! `just run pane_grid` (or `just run-hot pane_grid` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    pane_grid::main_app().run(Size::new(600.0, 420.0));
}
