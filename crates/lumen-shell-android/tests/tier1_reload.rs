//! T3.2 acceptance: tier-1 hot reload on the Android emulator. Pushing a new
//! stylesheet to the device changes the live UI without a rebuild/restart.
//!
//! `#[ignore]` (needs the emulator + SDK). The shell watches
//! `/data/local/tmp/lumen.lss`; we push one that paints `#screen` red and assert
//! the on-device frame gains red pixels it did not have before.

use lumen_render::RgbaImage;
use std::process::Command;

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

fn screencap() -> RgbaImage {
    let png = sh("adb", &["exec-out", "screencap", "-p"]).expect("screencap");
    RgbaImage::from_png(&png).expect("decode")
}

/// Count strongly-red pixels (a proxy for the reloaded background).
fn red_pixels(img: &RgbaImage) -> usize {
    let p = img.pixels();
    let mut n = 0;
    for px in p.chunks_exact(4) {
        if px[0] > 150 && px[1] < 90 && px[2] < 90 {
            n += 1;
        }
    }
    n
}

#[test]
#[ignore = "requires a running Android emulator + SDK; run with --ignored"]
fn tier1_stylesheet_reload_changes_device_ui() {
    if !emulator_ready() {
        eprintln!("no emulator/SDK; skipping tier-1 reload");
        return;
    }
    let root = env!("CARGO_MANIFEST_DIR");
    let apk = "/tmp/lumen_hello_t32.apk";

    // The app reads its external files dir (adb-writable, app-readable).
    let lss_dir = "/sdcard/Android/data/dev.lumen.hello/files";
    let lss_dev = format!("{lss_dir}/lumen.lss");

    // Clean slate: remove any stale pushed stylesheet, build, install, launch.
    sh("adb", &["shell", "rm", "-f", &lss_dev]);
    let built = Command::new("bash")
        .args([
            &format!("{root}/../../scripts/android_build_apk.sh"),
            "hello_android",
            "hello_android",
            "x86_64",
            apk,
        ])
        .status()
        .expect("build");
    assert!(built.success());
    assert!(sh("adb", &["install", "-r", "-t", apk]).is_some());
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
    sh("adb", &["shell", "mkdir", "-p", lss_dir]);
    std::thread::sleep(std::time::Duration::from_secs(4));

    let before = red_pixels(&screencap());

    // Push a stylesheet that paints the root red — the dev-loop "edit".
    std::fs::write(
        "/tmp/lumen_reload.lss",
        "#screen { background: #cc1111; }\n",
    )
    .unwrap();
    assert!(sh("adb", &["push", "/tmp/lumen_reload.lss", &lss_dev]).is_some());
    std::thread::sleep(std::time::Duration::from_secs(3));

    let after = red_pixels(&screencap());
    assert!(
        after > before + 1000,
        "tier-1 reload did not paint the UI red (before={before}, after={after})"
    );
}
