//! Renders pane_grid to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = pane_grid::main_app().run_headless(Size::new(600.0, 420.0));
    let s = a.pump();
    std::fs::write("/tmp/pane_grid.png", a.screenshot().to_png()).unwrap();
    println!("pane_grid: {} nodes -> /tmp/pane_grid.png", s.node_count);
}
