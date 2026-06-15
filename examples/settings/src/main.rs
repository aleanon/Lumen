//! `settings` binary: a headless smoke run (the winit shell drives the window).

use lumen_core::geometry::Size;

fn main() {
    let mut app = settings::main_app().run_headless(Size::new(480.0, 360.0));
    let stats = app.pump();
    println!("settings: rendered {} nodes", stats.node_count);
}
