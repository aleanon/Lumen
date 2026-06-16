//! `cargo run -p iced-parity --example win -- <name>` opens an iced-parity
//! example in a real interactive desktop window (winit + wgpu, via
//! `lumen-shell`). Blocks until the window is closed. Wired to `just win <name>`.
//! `win list` prints the available names.
//!
//! This is the windowed counterpart to the headless `show` target; both share
//! [`iced_parity::app_for`] so they expose exactly the same gallery.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_default();
    if name.is_empty() || name == "list" {
        println!("iced-parity examples:");
        for n in iced_parity::EXAMPLES {
            println!("  {n}");
        }
        return;
    }
    match iced_parity::app_for(&name) {
        // `run` opens the window and drives the event loop until close.
        Some(app) => app.run(Size::new(480.0, 360.0)),
        None => {
            eprintln!(
                "unknown example '{name}'. try: cargo run -p iced-parity --example win -- list"
            );
            std::process::exit(1);
        }
    }
}
