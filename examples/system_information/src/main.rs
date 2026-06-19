//! Renders system_information to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = system_information::main_app().run_headless(Size::new(520.0, 460.0));
    let s = a.pump();
    std::fs::write("/tmp/system_information.png", a.screenshot().to_png()).unwrap();
    println!(
        "system_information: {} nodes -> /tmp/system_information.png",
        s.node_count
    );
}
