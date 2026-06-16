//! `cargo run -p iced-parity --example show -- <name>` renders one iced-parity
//! example one frame headless and writes a PNG you can open. `show list` prints
//! the names. Wired to `just run <name>`. For an interactive window instead, see
//! the `win` target (`just win <name>`).
use lumen_core::geometry::Size;

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
        Some(app) => {
            let mut h = app.run_headless(Size::new(480.0, 360.0));
            let path = format!("/tmp/lumen-{name}.png");
            std::fs::write(&path, h.screenshot().to_png()).unwrap();
            println!(
                "rendered '{name}' ({} nodes) -> {path}",
                h.semantics_doc().root.elided().children.len()
            );
        }
        None => {
            eprintln!(
                "unknown example '{name}'. try: cargo run -p iced-parity --example show -- list"
            );
            std::process::exit(1);
        }
    }
}
