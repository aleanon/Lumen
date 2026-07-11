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
            // C.8b: run the `#[lumen_test::test(platform(gpu))]`-ignored
            // tests against the GPU renderer. Convention: their names
            // contain `gpu` (that's cargo's positional filter below).
            Some("gpu") => cmd_gpu_test(json),
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
        // C.5/C.8b: the packaged agent client + MCP stdio server + serve.
        Some("agent") => match positional.get(1).copied() {
            Some("call") => lumen_cli::agent::cmd_call(
                positional.get(2).copied(),
                positional.get(3).copied(),
                json,
            ),
            Some("mcp") => lumen_cli::agent::cmd_mcp(),
            Some("serve") => lumen_cli::agent::cmd_serve(),
            _ => fail(json, "agent", "usage: lumen agent <call|mcp|serve> …"),
        },
        // C.8b: human-oriented view of the running app.
        Some("inspect") => lumen_cli::agent::cmd_inspect(positional.get(1).copied(), json),
        // C.7: the tier-2/3 live dev host — build, load, watch, swap.
        Some("dev") => cmd_dev(positional.get(1).copied(), positional.get(2).copied(), json),
        Some(other) => fail(json, "usage", &format!("unknown command `{other}`")),
        None => fail(
            json,
            "usage",
            "usage: lumen <new|run|test|package|add|agent|inspect|dev> [--platform <p>] [--json]",
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

/// Note prepended (after the frontmatter) to skills copied into a scaffold,
/// re-grounding framework-repo paths/idioms for a standalone app.
const SKILL_SCAFFOLD_NOTE: &str = "\
> **Scaffolded copy** (shipped by `lumen new`). In this project, import\n\
> through the `lumen` facade (`use lumen::widgets::…`) instead of the\n\
> internal `lumen_core`/`lumen_widgets` crates the framework repo's\n\
> examples use (ADR-W2). `scripts/agent_client.py` and the `justfile` are\n\
> included here (`just run-agent` enables the endpoint via the facade's\n\
> `agent` feature); paths like `.ai_docs/…`, `docs/…`, `examples/…`, and\n\
> `crates/…` refer to the Lumen framework repository.\n";

/// The four app-facing skills shipped into every scaffold (plan S0.8),
/// embedded at CLI build time so they version with the framework.
const SCAFFOLD_SKILLS: [(&str, &str); 4] = [
    (
        "building-apps",
        include_str!("../../../.claude/skills/building-apps/SKILL.md"),
    ),
    (
        "styling-lss",
        include_str!("../../../.claude/skills/styling-lss/SKILL.md"),
    ),
    (
        "verifying-apps",
        include_str!("../../../.claude/skills/verifying-apps/SKILL.md"),
    ),
    (
        "debugging-lumen",
        include_str!("../../../.claude/skills/debugging-lumen/SKILL.md"),
    ),
];

/// Insert the scaffold note directly after the skill's `---` frontmatter so
/// the frontmatter stays first (skill loaders require it).
fn with_scaffold_note(skill: &str) -> String {
    let close = "\n---\n";
    match skill[3..].find(close) {
        Some(i) => {
            let split = 3 + i + close.len();
            format!(
                "{}\n{SKILL_SCAFFOLD_NOTE}{}",
                &skill[..split],
                &skill[split..]
            )
        }
        None => format!("{SKILL_SCAFFOLD_NOTE}\n{skill}"),
    }
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

    // Windowed entry (desktop): `just run` / `just run-agent`.
    let win_rs = format!(
        "//! Windowed entry: `just run` (or `cargo run --release --example win`).\n\
         use lumen::geometry::Size;\n\
         use lumen::RunExt;\n\n\
         fn main() {{\n    \
            {name}::main_app().run(Size::new(800.0, 600.0));\n\
         }}\n"
    );
    // Task runner mirroring the framework repo's recipes the skills teach.
    let justfile = "# `just` lists recipes. Release builds: debug rendering is ~35x slower.\n\n\
        # Open the app in a desktop window.\n\
        run:\n    cargo run --release --example win\n\n\
        # Window + agent endpoint (JSON-RPC) so an AI can observe and drive it.\n\
        # See .claude/skills/verifying-apps and scripts/agent_client.py.\n\
        run-agent addr=\"127.0.0.1:9230\":\n    \
        LUMEN_AGENT_ADDR={{addr}} cargo run --release --example win --features lumen/agent\n\n\
        # Headless tests (CI-safe, no display needed).\n\
        test:\n    cargo test\n";

    if let Err(e) = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("src"))?;
        std::fs::create_dir_all(dir.join("tests"))?;
        std::fs::create_dir_all(dir.join("examples"))?;
        std::fs::create_dir_all(dir.join("scripts"))?;
        std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;
        std::fs::write(dir.join("src/lib.rs"), lib_rs)?;
        std::fs::write(dir.join("src/main.rs"), main_rs)?;
        std::fs::write(dir.join("tests/app.rs"), test_rs)?;
        std::fs::write(dir.join("examples/win.rs"), win_rs)?;
        std::fs::write(dir.join("justfile"), justfile)?;
        std::fs::write(
            dir.join("scripts/agent_client.py"),
            include_str!("../../../scripts/agent_client.py"),
        )?;
        // The app-facing skill suite, so an agent opening this project knows
        // how to build, style, verify, and debug it (plan S0.8).
        for (skill, body) in SCAFFOLD_SKILLS {
            let d = dir.join(".claude/skills").join(skill);
            std::fs::create_dir_all(&d)?;
            std::fs::write(d.join("SKILL.md"), with_scaffold_note(body))?;
        }
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
    cmd_passthrough_env(command, cargo_args, &[], json)
}

/// C.7 `lumen dev <component-crate> <watch-path>`: build the component
/// cdylib, host it in the live tier-2 driver, and rebuild+swap on every
/// change (ABI mismatch downgrades to a tier-3 snapshot restart). Emits one
/// `ReloadResult`-shaped JSON line per applied build.
fn cmd_dev(krate: Option<&str>, watch: Option<&str>, json: bool) -> i32 {
    let (Some(krate), Some(watch)) = (krate, watch) else {
        return fail(
            json,
            "dev",
            "usage: lumen dev <component-crate> <watch-path>",
        );
    };
    let dylib = match lumen_cli::dev::Tier2Driver::build_component(krate) {
        Ok(p) => p,
        Err(e) => return fail(json, "dev", &e),
    };
    let mut driver = match lumen_cli::dev::Tier2Driver::start(
        &dylib,
        lumen_core::geometry::Size::new(800.0, 600.0),
    ) {
        Ok(d) => d,
        Err(e) => return fail(json, "dev", &e),
    };
    eprintln!("lumen dev: hosting {krate}; watching {watch} (tier-2 swap, tier-3 on ABI change)");
    match driver.watch_and_apply(krate, std::path::Path::new(watch)) {
        Ok(()) => 0,
        Err(e) => fail(json, "dev", &e.to_string()),
    }
}

/// C.8b `lumen test --platform gpu`: run the GPU-platform tests (ignored by
/// default; names contain `gpu`) with the GPU renderer selected.
fn cmd_gpu_test(json: bool) -> i32 {
    cmd_passthrough_env(
        "test",
        &["test", "--workspace", "--", "--ignored", "gpu"],
        &[("LUMEN_RENDERER", "wgpu")],
        json,
    )
}

fn cmd_passthrough_env(
    command: &str,
    cargo_args: &[&str],
    envs: &[(&str, &str)],
    json: bool,
) -> i32 {
    let mut cmd = Command::new("cargo");
    cmd.args(cargo_args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    match cmd.output() {
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
