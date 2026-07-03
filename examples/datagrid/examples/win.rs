//! `just run datagrid` (or `just run-hot datagrid` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    datagrid::main_app().run(Size::new(1000.0, 700.0));
}
