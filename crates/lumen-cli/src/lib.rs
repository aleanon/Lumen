//! `lumen-cli` library surface — the dev server + tier-1 hot reload (T1.7).
//!
//! The CLI binary (`src/main.rs`) handles `new`/`run`/`test`; this lib hosts the
//! dev-server pieces that are unit/integration tested.
#![warn(missing_docs)]

pub mod agent;
pub mod dev;
pub mod dist;
pub mod hotpatch;
pub mod proto;

// --- E.2: crates.io version resolution for `lumen add` -----------------------

/// Resolve the latest stable version of `krate` from the crates.io API
/// (via curl; `None` offline/unknown). The plugin story is source-level
/// (ADR-W1: `LeafWidget` is the stable 1.x API, not an ABI) — "registering"
/// a widget IS adding the dependency and calling its constructor.
pub fn resolve_crate_version(krate: &str) -> Option<String> {
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "10",
            "-H",
            "User-Agent: lumen-cli (https://example.invalid/lumen)",
        ])
        .arg(format!("https://crates.io/api/v1/crates/{krate}"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_max_version(&out.stdout)
}

/// Extract `crate.max_stable_version` from a crates.io API response.
pub fn parse_max_version(body: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    v["crate"]["max_stable_version"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "null")
        .map(String::from)
}
