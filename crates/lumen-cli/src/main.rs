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

    let platform = flag_value(&args, "--platform");
    match positional.first().copied() {
        Some("new") => cmd_new(positional.get(1).copied(), json),
        Some("test") => match platform.as_deref() {
            Some(p @ ("android" | "ios_sim" | "web")) => cmd_mobile("test", p, json),
            Some(other) => fail(json, "test", &format!("unknown platform `{other}`")),
            None => cmd_passthrough("test", &["test"], json),
        },
        Some("run") => match platform.as_deref() {
            Some(p @ ("android" | "ios_sim" | "web")) => cmd_mobile("run", p, json),
            Some(other) => fail(json, "run", &format!("unknown platform `{other}`")),
            None => cmd_passthrough("run", &["run"], json),
        },
        Some("package") => cmd_package(json),
        Some("add") => cmd_add(positional.get(1).copied(), json),
        Some(other) => fail(json, "usage", &format!("unknown command `{other}`")),
        None => fail(
            json,
            "usage",
            "usage: lumen <new|run|test|package> [--platform <p>] [--json]",
        ),
    }
}

/// `lumen add <crate>`: append a widget/plugin dependency to the current
/// crate's `Cargo.toml` (T7.2 ecosystem).
fn cmd_add(krate: Option<&str>, json: bool) -> i32 {
    let Some(krate) = krate else {
        return fail(json, "add", "usage: lumen add <crate>");
    };
    let toml = match std::fs::read_to_string("Cargo.toml") {
        Ok(t) => t,
        Err(_) => return fail(json, "add", "no Cargo.toml here"),
    };
    if toml.contains(&format!("\n{krate} =")) {
        return ok(
            json,
            "add",
            json!({ "crate": krate, "added": false }),
            &format!("{krate} already a dependency"),
        );
    }
    let line = format!("{krate} = \"*\"\n");
    let updated = if let Some(i) = toml.find("[dependencies]") {
        let nl = toml[i..]
            .find('\n')
            .map(|n| i + n + 1)
            .unwrap_or(toml.len());
        format!("{}{}{}", &toml[..nl], line, &toml[nl..])
    } else {
        format!("{toml}\n[dependencies]\n{line}")
    };
    if std::fs::write("Cargo.toml", updated).is_err() {
        return fail(json, "add", "could not write Cargo.toml");
    }
    ok(
        json,
        "add",
        json!({ "crate": krate, "added": true }),
        &format!("added dependency {krate}"),
    )
}

/// `lumen package`: build the current crate in release and write a portable
/// bundle (binary + assets + manifest). Code signing / installer formats are a
/// per-OS step on top of the bundle (T7.1).
fn cmd_package(json: bool) -> i32 {
    let toml = match std::fs::read_to_string("Cargo.toml") {
        Ok(t) => t,
        Err(_) => return fail(json, "package", "no Cargo.toml in the current directory"),
    };
    let field = |key: &str| {
        toml.lines()
            .find_map(|l| l.trim().strip_prefix(key))
            .and_then(|r| r.split('"').nth(1))
            .unwrap_or("app")
            .to_string()
    };
    let name = field("name =").replace('"', "");
    let version = field("version =");

    let built = Command::new("cargo").args(["build", "--release"]).status();
    match built {
        Ok(s) if s.success() => {}
        _ => return fail(json, "package", "release build failed"),
    }
    let bin = Path::new("target/release").join(&name);
    let bytes = match std::fs::read(&bin) {
        Ok(b) => b,
        Err(_) => return fail(json, "package", &format!("no release binary at {bin:?}")),
    };
    let platform = std::env::consts::OS;
    let manifest = lumen_cli::dist::BundleManifest::new(&name, &version, platform, &name);
    match lumen_cli::dist::package(Path::new("target"), manifest, &bytes, &[]) {
        Ok(dir) => ok(
            json,
            "package",
            json!({ "bundle": dir.display().to_string(), "platform": platform }),
            &format!("packaged {name} {version} → {}", dir.display()),
        ),
        Err(e) => fail(json, "package", &format!("bundle failed: {e}")),
    }
}

/// Value of `--flag value` or `--flag=value`, if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(v) = a.strip_prefix(&format!("{flag}=")) {
            return Some(v.to_string());
        }
        if a == flag {
            return it.next().cloned();
        }
    }
    None
}

/// `lumen <run|test> --platform <android|ios_sim>`: delegate to the platform
/// orchestration script, which provisions the device, builds, deploys, and (for
/// `run`) wires the dev socket / streams logs (T3.2/T3.4/T3.6).
fn cmd_mobile(command: &str, platform: &str, json: bool) -> i32 {
    let script = match platform {
        "android" => "scripts/android_orchestrate.sh",
        "ios_sim" => "scripts/ios_orchestrate.sh",
        "web" => "scripts/web_orchestrate.sh",
        _ => unreachable!(),
    };
    if !Path::new(script).exists() {
        return fail(
            json,
            command,
            &format!("missing orchestration script {script}"),
        );
    }
    let status = Command::new("bash").arg(script).arg(command).status();
    match status {
        Ok(s) if s.success() => ok(
            json,
            command,
            json!({ "platform": platform }),
            &format!("{command} on {platform}: ok"),
        ),
        Ok(s) => fail(
            json,
            command,
            &format!("{platform} {command} failed (exit {:?})", s.code()),
        ),
        Err(e) => fail(json, command, &format!("could not run {script}: {e}")),
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
