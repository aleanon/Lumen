//! `just run widget_showcase` (or `just run-agent widget_showcase` to drive it).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    widget_showcase::main_app().run(Size::new(980.0, 760.0));
}
