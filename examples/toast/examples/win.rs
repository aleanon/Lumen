//! `just win toast` (or `just win-watch toast examples/toast/app.lss`).
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    toast::main_app().run(Size::new(520.0, 460.0));
}
