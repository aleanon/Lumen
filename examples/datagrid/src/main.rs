//! Renders the data grid to a PNG (and is the binary entry point / headless smoke).
use lumen_core::geometry::Size;

fn main() {
    let mut a = datagrid::main_app().run_headless(Size::new(1000.0, 700.0));
    let stats = a.pump();
    std::fs::write("/tmp/datagrid.png", a.screenshot().to_png()).unwrap();
    println!("datagrid: {} nodes -> /tmp/datagrid.png", stats.node_count);
}
