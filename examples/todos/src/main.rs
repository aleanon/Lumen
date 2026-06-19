//! Renders the todos app to a PNG (and is the binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = todos::main_app().run_headless(Size::new(520.0, 520.0));
    let stats = a.pump();
    std::fs::write("/tmp/todos.png", a.screenshot().to_png()).unwrap();
    println!("todos: {} nodes -> /tmp/todos.png", stats.node_count);
}
