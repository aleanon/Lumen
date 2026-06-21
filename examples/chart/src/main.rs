//! Renders chart to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = chart::main_app().run_headless(Size::new(600.0, 640.0));
    let s = a.pump();
    std::fs::write("/tmp/chart.png", a.screenshot().to_png()).unwrap();
    println!("chart: {} nodes -> /tmp/chart.png", s.node_count);
}
