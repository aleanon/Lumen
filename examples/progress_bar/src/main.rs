//! Renders progress_bar to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = progress_bar::main_app().run_headless(Size::new(480.0, 500.0));
    let s = a.pump();
    std::fs::write("/tmp/progress_bar.png", a.screenshot().to_png()).unwrap();
    println!(
        "progress_bar: {} nodes -> /tmp/progress_bar.png",
        s.node_count
    );
}
