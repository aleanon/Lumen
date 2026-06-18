//! `cargo run -p chrono-stopwatch --example win` opens PULSE in a real
//! interactive desktop window (winit + wgpu, via `lumen-shell`). Blocks until
//! the window is closed. Wired to `just win chrono-stopwatch`.

use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    chrono_stopwatch::main_app().run(Size::new(460.0, 600.0));
}
