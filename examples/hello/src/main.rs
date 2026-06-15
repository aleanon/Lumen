//! `hello` binary entry point.
//!
//! Until the winit shell lands (T0.11), this runs one headless frame as a smoke
//! check. `lumen run` will drive the real window.

use lumen::geometry::Size;

fn main() {
    let mut app = hello::main_app().run_headless(Size::new(800.0, 600.0));
    let stats = app.pump();
    println!(
        "lumen hello: rendered {} nodes headlessly",
        stats.node_count
    );
}
