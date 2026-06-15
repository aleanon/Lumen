//! The `lumen` developer CLI (01 §8): `new`, `run`, `test`.
//!
//! Every command supports `--json`, which suppresses human output and prints a
//! single result envelope `{ "ok": bool, "command": ..., ... }` to stdout.
//!
//! No `clap` (outside the ADR-003 whitelist): args are parsed by hand.

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let positional: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    match positional.first().copied() {
        Some("new") => cmd_new(positional.get(1).copied(), json),
        Some("test") => cmd_passthrough("test", &["test"], json),
        Some("run") => cmd_passthrough("run", &["run"], json),
        Some(other) => fail(json, "usage", &format!("unknown command `{other}`")),
        None => fail(json, "usage", "usage: lumen <new|run|test> [--json]"),
    }
}

/// Emit a success envelope (or human text) and return exit 0.
fn ok(json: bool, command: &str, data: Value, human: &str) -> i32 {
    if json {
        println!(
            "{}",
            json!({ "ok": true, "command": command, "data": data })
        );
    } else {
        println!("{human}");
    }
    0
}

/// Emit a failure envelope (or human text) and return exit 2.
fn fail(json: bool, command: &str, message: &str) -> i32 {
    if json {
        println!(
            "{}",
            json!({ "ok": false, "command": command, "error": message })
        );
    } else {
        eprintln!("error: {message}");
    }
    2
}

/// `lumen new <name>`: scaffold an app crate using the `main_app()` convention.
fn cmd_new(name: Option<&str>, json: bool) -> i32 {
    let Some(name) = name else {
        return fail(json, "new", "usage: lumen new <name>");
    };
    let dir = Path::new(name);
    if dir.exists() {
        return fail(json, "new", &format!("`{name}` already exists"));
    }

    // Path deps in local dev (LUMEN_LOCAL_PATH=<workspace>), else placeholders.
    let (lumen_dep, lumen_test_dep) = match std::env::var("LUMEN_LOCAL_PATH") {
        Ok(ws) => (
            format!("lumen = {{ path = {:?} }}", format!("{ws}/crates/lumen")),
            format!(
                "lumen-test = {{ path = {:?} }}",
                format!("{ws}/crates/lumen-test")
            ),
        ),
        Err(_) => (
            "lumen = \"0.0.0\"".to_string(),
            "lumen-test = \"0.0.0\"".to_string(),
        ),
    };

    let cargo_toml = format!(
        "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [lib]\nname = {name:?}\npath = \"src/lib.rs\"\n\n\
         [[bin]]\nname = {name:?}\npath = \"src/main.rs\"\n\n\
         [dependencies]\n{lumen_dep}\n\n[dev-dependencies]\n{lumen_test_dep}\n"
    );
    let lib_rs = "//! A Lumen app.\n\
        use lumen::widgets::{button, column, text};\n\
        use lumen::App;\n\n\
        /// The application entry point (`lumen` convention).\n\
        pub fn main_app() -> App {\n    \
            App::new(|cx| {\n        \
                let count = cx.signal(\"count\", || 0i32);\n        \
                let value = count.get(cx.runtime());\n        \
                column(vec![\n            \
                    text(format!(\"Count: {value}\")).id(\"count\"),\n            \
                    button(\"+1\", move |rt| count.update(rt, |c| *c += 1)).id(\"increment\"),\n        \
                ])\n    \
            })\n}\n";
    let main_rs = format!(
        "//! `{name}` binary.\n\
         use lumen::geometry::Size;\n\n\
         fn main() {{\n    \
            let mut app = {name}::main_app().run_headless(Size::new(800.0, 600.0));\n    \
            let stats = app.pump();\n    \
            println!(\"rendered {{}} nodes\", stats.node_count);\n}}\n"
    );
    let test_rs = format!(
        "use lumen_test::{{block_on, expect, TestApp}};\n\n\
         #[test]\n\
         fn counter_increments() {{\n    \
            block_on(async {{\n        \
                let mut app = TestApp::new({name}::main_app());\n        \
                app.pump_until_idle().await;\n        \
                app.locator(\"#increment\").click().await.unwrap();\n        \
                expect(app.locator(\"#count\")).to_have_text(\"Count: 1\").await.unwrap();\n    \
            }});\n}}\n"
    );

    if let Err(e) = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("src"))?;
        std::fs::create_dir_all(dir.join("tests"))?;
        std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;
        std::fs::write(dir.join("src/lib.rs"), lib_rs)?;
        std::fs::write(dir.join("src/main.rs"), main_rs)?;
        std::fs::write(dir.join("tests/app.rs"), test_rs)?;
        Ok(())
    })() {
        return fail(json, "new", &format!("could not scaffold: {e}"));
    }

    ok(
        json,
        "new",
        json!({ "name": name, "path": dir.display().to_string() }),
        &format!("created `{name}` (run `cd {name} && lumen test`)"),
    )
}

/// `lumen test` / `lumen run`: wrap the corresponding cargo command.
fn cmd_passthrough(command: &str, cargo_args: &[&str], json: bool) -> i32 {
    match Command::new("cargo").args(cargo_args).output() {
        Ok(out) => {
            let success = out.status.success();
            if json {
                let tail = |b: &[u8]| {
                    let s = String::from_utf8_lossy(b);
                    let lines: Vec<&str> = s.lines().collect();
                    let start = lines.len().saturating_sub(20);
                    lines[start..].join("\n")
                };
                println!(
                    "{}",
                    json!({
                        "ok": success,
                        "command": command,
                        "data": {
                            "exit_code": out.status.code(),
                            "stdout_tail": tail(&out.stdout),
                            "stderr_tail": tail(&out.stderr),
                        }
                    })
                );
            } else {
                print!("{}", String::from_utf8_lossy(&out.stdout));
                eprint!("{}", String::from_utf8_lossy(&out.stderr));
            }
            i32::from(!success)
        }
        Err(e) => fail(json, command, &format!("failed to run cargo: {e}")),
    }
}
