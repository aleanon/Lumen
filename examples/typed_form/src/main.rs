//! Renders typed_form to a PNG (binary entry point).
use lumen_core::geometry::Size;

fn main() {
    let mut a = typed_form::main_app().run_headless(Size::new(460.0, 560.0));
    let s = a.pump();
    std::fs::write("/tmp/typed_form.png", a.screenshot().to_png()).unwrap();
    println!("typed_form: {} nodes -> /tmp/typed_form.png", s.node_count);
}
