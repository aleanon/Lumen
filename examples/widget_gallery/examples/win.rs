//! `just run widget_gallery` (or `just run-hot widget_gallery` for live `.lss` reload).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    widget_gallery::main_app().run(Size::new(620.0, 980.0));
}
