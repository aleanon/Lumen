//! `just run download_progress`.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    download_progress::main_app().run(Size::new(420.0, 320.0));
}
