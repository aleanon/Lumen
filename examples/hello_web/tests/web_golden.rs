//! T5.1 acceptance: the hello app, compiled to WASM and run under node (no
//! browser), renders the **same pixels** as the native CPU reference renderer —
//! the cross-platform golden-parity guarantee on the web.
//!
//! `#[ignore]` because it builds the wasm target and shells out to node. Real
//! in-browser WebGPU rendering + agent-over-WebSocket is a separate, browser-CI
//! leg (authored in `web/`), not runnable on this headless box.

use lumen::{widgets, App, BuildCx, Element};
use lumen_render::RgbaImage;
use std::process::Command;

fn hello(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}

fn have_node() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "builds the wasm target + needs node; run with --ignored"]
fn wasm_render_matches_native_cpu() {
    if !have_node() {
        eprintln!("node missing; skipping wasm golden");
        return;
    }
    let root = env!("CARGO_MANIFEST_DIR");
    let (w, h) = (240u32, 80u32);

    // Build the wasm module.
    let built = Command::new("cargo")
        .args([
            "build",
            "-p",
            "hello_web",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .current_dir(format!("{root}/../.."))
        .status()
        .expect("cargo build wasm");
    assert!(built.success());

    // Render it under node, capturing the raw RGBA frame.
    let wasm = format!("{root}/../../target/wasm32-unknown-unknown/release/hello_web.wasm");
    let out = "/tmp/lumen_web_golden.rgba";
    let r = Command::new("node")
        .args([
            &format!("{root}/web/render.mjs"),
            &wasm,
            &w.to_string(),
            &h.to_string(),
            out,
        ])
        .status()
        .expect("node render");
    assert!(r.success());
    let wasm_px = std::fs::read(out).expect("rgba");
    assert_eq!(wasm_px.len(), (w * h * 4) as usize);

    // Render the same app natively on the CPU reference renderer.
    let native: RgbaImage = App::new(hello)
        .run_headless(lumen::geometry::Size::new(w as f64, h as f64))
        .screenshot();

    // Deterministic CPU renderer → identical bytes on wasm and native.
    let diff = wasm_px
        .chunks_exact(4)
        .zip(native.pixels().chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        diff, 0,
        "wasm vs native CPU render differs at {diff} pixels"
    );
}
