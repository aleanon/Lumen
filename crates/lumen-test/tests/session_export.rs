//! T2.4/T2.5 acceptance: a recorded session exports a compiling, passing
//! lumen-test. The fast tests record live and check the exported source shape;
//! the `#[ignore]`d test actually compiles + runs the exported file.

use lumen_test::{block_on, Session, TestApp};
use lumen_widgets::{widgets, App};
use std::path::PathBuf;
use std::process::Command;

/// The app under test — also emitted verbatim into the exported file's header.
const HEADER: &str = r#"use lumen::{widgets, App};

fn counter() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("Count: {v}")).id("count"),
            widgets::button("+1", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        ])
    })
}
"#;

fn counter() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let v = count.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("Count: {v}")).id("count"),
            widgets::button("+1", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        ])
    })
}

/// Drive a real session and return its exported source.
fn record_export() -> String {
    block_on(async {
        let mut s = Session::new(TestApp::new(counter()), "counter()");
        s.click("#inc").await.unwrap();
        s.click("#inc").await.unwrap();
        // The assertion is part of the session: it ran live here and is replayed
        // in the exported test.
        s.expect_text("#count", "Count: 2").await.unwrap();
        assert_eq!(s.len(), 3);
        s.export_test("recorded_session", HEADER)
    })
}

#[test]
fn export_emits_expected_source() {
    let src = record_export();
    assert!(src.contains("fn recorded_session()"));
    assert!(src.contains("lumen_test::TestApp::new(counter())"));
    assert_eq!(src.matches(r##".locator("#inc").click()"##).count(), 2);
    assert!(src.contains(r#"to_have_text("Count: 2")"#));
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Compile + run the exported test in a throwaway crate. `#[ignore]` because it
/// builds the framework in a fresh target dir (slow); run explicitly/in CI.
#[test]
#[ignore = "compiles the framework in a throwaway target; run explicitly/in CI"]
fn exported_test_compiles_and_passes() {
    let src = record_export();
    let ws = workspace_root();
    let dir = std::env::temp_dir().join(format!("lumen-session-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("tests")).unwrap();

    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"exported\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n\
             lumen = {{ path = \"{ws}/crates/lumen\" }}\n\
             lumen-test = {{ path = \"{ws}/crates/lumen-test\" }}\n",
            ws = ws.display()
        ),
    )
    .unwrap();
    std::fs::write(dir.join("tests/recorded.rs"), &src).unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["test", "--test", "recorded"])
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("run cargo test on exported crate");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "exported test failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("test result: ok"), "stdout:\n{stdout}");
    std::fs::remove_dir_all(&dir).ok();
}
