//! `just win pane_grid` (or `just win-watch pane_grid examples/pane_grid/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    pane_grid::main_app().run(Size::new(600.0, 420.0));
}
