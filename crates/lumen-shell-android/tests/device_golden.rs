//! T3.1 acceptance: the hello app runs on an Android emulator and its on-device
//! screenshot matches the headless CPU reference perceptually.
//!
//! `#[ignore]` because it needs a running emulator + the SDK/NDK (source
//! `android-env.sh` first). The device frame is a software blit of the very CPU
//! frame this test renders headless, so the two should match within the
//! ADR-002 perceptual budget (ΔE Oklab ≤ 2.0 on ≤ 0.1% of pixels).

use kurbo::Size;
use lumen::{widgets, App, BuildCx, Element};
use lumen_core::Color;
use lumen_render::RgbaImage;
use std::process::Command;

/// The same hello app the cdylib runs (kept in sync with `hello_android::app`).
fn hello(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}

fn sh(cmd: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn emulator_ready() -> bool {
    std::env::var("ANDROID_HOME").is_ok()
        && sh("adb", &["devices"])
            .map(|o| String::from_utf8_lossy(&o).contains("\tdevice"))
            .unwrap_or(false)
}

#[test]
#[ignore = "requires a running Android emulator + SDK; source android-env.sh and run --ignored"]
fn hello_renders_on_device_matching_headless() {
    if !emulator_ready() {
        eprintln!("no emulator/SDK; skipping device golden");
        return;
    }
    let root = env!("CARGO_MANIFEST_DIR");
    let apk = "/tmp/lumen_hello_t31.apk";

    // Build, install, launch.
    let build = Command::new("bash")
        .args([
            &format!("{root}/../../scripts/android_build_apk.sh"),
            "hello_android",
            "hello_android",
            "x86_64",
            apk,
        ])
        .status()
        .expect("run build script");
    assert!(build.success(), "APK build failed");
    assert!(
        sh("adb", &["install", "-r", "-t", apk]).is_some(),
        "install failed"
    );
    sh("adb", &["shell", "am", "force-stop", "dev.lumen.hello"]);
    sh(
        "adb",
        &[
            "shell",
            "am",
            "start",
            "-n",
            "dev.lumen.hello/android.app.NativeActivity",
        ],
    )
    .expect("launch");
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Capture the device frame.
    let png = sh("adb", &["exec-out", "screencap", "-p"]).expect("screencap");
    let shot = RgbaImage::from_png(&png).expect("decode screencap");
    let (w, h) = (shot.width(), shot.height());

    // Render the identical app headless at the device resolution.
    let reference = App::new(hello)
        .run_headless(Size::new(w as f64, h as f64))
        .screenshot();

    // Compare the top 80% (excludes the system nav bar) perceptually.
    let region_h = h * 8 / 10;
    let (sp, rp) = (shot.pixels(), reference.pixels());
    let mut bad = 0usize;
    for y in 0..region_h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = Color::srgb8(sp[i], sp[i + 1], sp[i + 2], 255);
            let b = Color::srgb8(rp[i], rp[i + 1], rp[i + 2], 255);
            if a.delta_e_oklab(b) > 2.0 {
                bad += 1;
            }
        }
    }
    let frac = bad as f64 / (w * region_h) as f64;
    assert!(
        frac <= 0.001,
        "perceptual mismatch: {:.4}% of pixels",
        frac * 100.0
    );
}
