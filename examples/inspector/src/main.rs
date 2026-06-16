//! `inspector` binary — renders one headless frame of the Lumen inspector.
use lumen_core::geometry::Size;

fn main() {
    let mut app = inspector::main_app().run_headless(Size::new(720.0, 520.0));
    let stats = app.pump();
    println!("inspector rendered {} nodes", stats.node_count);
}
