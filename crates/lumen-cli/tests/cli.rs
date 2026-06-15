//! T0.12 acceptance: the CLI scaffolds an app and emits valid `--json`
//! envelopes. The full `lumen new demo && lumen test` end-to-end build is
//! `#[ignore]`d (it compiles the framework in a throwaway target — slow); the
//! default tests cover scaffolding and the JSON envelope contract.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn lumen() -> &'static str {
    env!("CARGO_BIN_EXE_lumen")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lumen-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn new_scaffolds_app_and_emits_json() {
    let dir = tmpdir("new");
    let out = Command::new(lumen())
        .args(["new", "demo", "--json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success());

    let env: Value = serde_json::from_slice(&out.stdout).expect("valid JSON envelope");
    assert_eq!(env["ok"], Value::Bool(true));
    assert_eq!(env["command"], "new");

    let demo = dir.join("demo");
    assert!(demo.join("Cargo.toml").exists());
    assert!(demo.join("tests/app.rs").exists());
    let lib = std::fs::read_to_string(demo.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("fn main_app()"),
        "scaffold uses main_app convention"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_refuses_existing_directory() {
    let dir = tmpdir("exists");
    std::fs::create_dir_all(dir.join("demo")).unwrap();
    let out = Command::new(lumen())
        .args(["new", "demo", "--json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let env: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(env["ok"], Value::Bool(false));
    std::fs::remove_dir_all(&dir).ok();
}

/// `lumen test --json` wraps `cargo test` and reports its result. Verified on a
/// trivial crate (no framework build) so the envelope contract is fast to test.
#[test]
fn test_json_envelope_reflects_cargo_result() {
    for (body, expect_ok) in [("fn t() {}", true), ("fn t() { panic!() }", false)] {
        let dir = tmpdir("env");
        let krate = dir.join("k");
        std::fs::create_dir_all(krate.join("src")).unwrap();
        std::fs::write(
            krate.join("Cargo.toml"),
            "[package]\nname = \"k\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(krate.join("src/lib.rs"), format!("#[test]\n{body}\n")).unwrap();

        let out = Command::new(lumen())
            .args(["test", "--json"])
            .current_dir(&krate)
            .output()
            .unwrap();
        let env: Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
        assert_eq!(env["command"], "test");
        assert_eq!(
            env["ok"],
            Value::Bool(expect_ok),
            "body {body:?} -> ok should be {expect_ok}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Full end-to-end: scaffold against the local workspace and run its test.
/// `#[ignore]` because it builds the framework in a fresh target directory.
#[test]
#[ignore = "compiles the framework in a throwaway target; run explicitly/in CI"]
fn e2e_new_demo_and_test() {
    let dir = tmpdir("e2e");
    let ws = workspace_root();

    let out = Command::new(lumen())
        .args(["new", "demo", "--json"])
        .current_dir(&dir)
        .env("LUMEN_LOCAL_PATH", &ws)
        .output()
        .unwrap();
    assert!(out.status.success(), "new failed");

    let out = Command::new(lumen())
        .args(["test", "--json"])
        .current_dir(dir.join("demo"))
        // Separate target dir to avoid locking the parent cargo invocation.
        .env("CARGO_TARGET_DIR", dir.join("demo-target"))
        .output()
        .unwrap();
    let env: Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON from lumen test --json");
    assert_eq!(env["ok"], Value::Bool(true), "demo test should pass: {env}");

    std::fs::remove_dir_all(&dir).ok();
}
