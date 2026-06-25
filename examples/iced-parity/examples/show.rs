//! `cargo run -p iced-parity --example show -- <name>` renders one iced-parity
//! example one frame headless and writes a PNG you can open. `show list` prints
//! the names. Wired to `just run <name>`. For an interactive window instead, see
//! the `win` target (`just win <name>`).
//!
//! Pass `--wgpu` to rasterize on the GPU backend (linear-light blending — the
//! "real" picture the live window shows) instead of the default CPU reference
//! (gamma, deterministic). Each future backend gets its own flag.
use lumen_core::geometry::Size;
use lumen_widgets::{CpuRenderer, Renderer};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wgpu = args.iter().any(|a| a == "--wgpu");
    let name = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_default();

    if name.is_empty() || name == "list" {
        println!("iced-parity examples:");
        for n in iced_parity::EXAMPLES {
            println!("  {n}");
        }
        return;
    }
    let Some(app) = iced_parity::app_for(&name) else {
        eprintln!("unknown example '{name}'. try: cargo run -p iced-parity --example show -- list");
        std::process::exit(1);
    };

    // Select the rasterization backend. Both are boxed so there's one code path
    // (the App's renderer is generic; `with_renderer` erases it). The golden/test
    // path stays CPU; `--wgpu` is for eyeballing/exporting the real GPU look.
    let (renderer, tag): (Box<dyn Renderer>, &str) = if wgpu {
        match lumen_render::gpu::GpuRenderer::new() {
            Some(gpu) => (Box::new(gpu), "wgpu"),
            None => {
                eprintln!("--wgpu requested but no GPU adapter found; using the CPU renderer");
                (Box::new(CpuRenderer), "cpu")
            }
        }
    } else {
        (Box::new(CpuRenderer), "cpu")
    };

    let mut h = app
        .with_renderer(renderer)
        .run_headless(Size::new(480.0, 360.0));
    let suffix = if tag == "cpu" {
        String::new()
    } else {
        format!("-{tag}")
    };
    let path = format!("/tmp/lumen-{name}{suffix}.png");
    std::fs::write(&path, h.screenshot().to_png()).unwrap();
    println!(
        "rendered '{name}' ({} nodes, {tag}) -> {path}",
        h.semantics_doc().root.elided().children.len()
    );
}
