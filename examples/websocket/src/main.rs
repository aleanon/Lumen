//! Renders websocket to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = websocket::main_app().run_headless(Size::new(520.0, 540.0));
    let s = a.pump();
    std::fs::write("/tmp/websocket.png", a.screenshot().to_png()).unwrap();
    println!("websocket: {} nodes -> /tmp/websocket.png", s.node_count);
}
