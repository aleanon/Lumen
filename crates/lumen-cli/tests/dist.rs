//! T7.1: bundle packaging produces a well-formed bundle + manifest.
use lumen_cli::dist::{package, BundleManifest};
use serde_json::Value;

#[test]
fn package_writes_bundle_and_manifest() {
    let tmp = std::env::temp_dir().join(format!("lumen-dist-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let m = BundleManifest::new("demo", "1.2.3", "linux", "demo");
    let assets = vec![("logo.svg".to_string(), b"<svg/>".to_vec())];
    let bundle = package(&tmp, m, b"ELF...binary...", &assets).unwrap();

    assert!(bundle.join("demo").exists(), "entry binary");
    assert!(bundle.join("assets/logo.svg").exists(), "asset");

    let manifest: Value =
        serde_json::from_slice(&std::fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema"], "lumen-bundle/1");
    assert_eq!(manifest["name"], "demo");
    assert_eq!(manifest["version"], "1.2.3");
    assert_eq!(manifest["platform"], "linux");
    assert_eq!(manifest["assets"][0], "logo.svg");

    std::fs::remove_dir_all(&tmp).ok();
}

// --- E.1 ---------------------------------------------------------------------

#[test]
fn sbom_lists_workspace_dependencies() {
    let pkgs = lumen_cli::dist::sbom(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert!(pkgs.len() > 50, "real dependency list: {}", pkgs.len());
    assert!(pkgs.iter().any(|p| p.name == "lumen-core"));
    assert!(pkgs
        .iter()
        .any(|p| p.name == "serde" && !p.license.is_empty()));
    // Deterministic ordering (sorted, deduped).
    let mut sorted = pkgs.clone();
    sorted.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    assert_eq!(
        pkgs.iter().map(|p| &p.name).collect::<Vec<_>>(),
        sorted.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
}

#[test]
fn appdir_wraps_the_bundle() {
    let tmp = std::env::temp_dir().join(format!("lumen-appdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let m = BundleManifest::new("demo", "1.0.0", "linux", "demo");
    let bundle = package(&tmp, m, b"#!/bin/sh\necho hi\n", &[]).unwrap();
    let appdir = lumen_cli::dist::build_appdir(&bundle, "demo").unwrap();
    assert!(appdir.join("AppRun").exists());
    assert!(appdir.join("demo.desktop").exists());
    assert!(appdir.join("demo.png").exists());
    assert!(appdir.join("usr/bin/demo").exists());
    assert!(appdir.join("usr/bin/manifest.json").exists());
    // AppRun is executable and actually runs the entry.
    let out = std::process::Command::new(appdir.join("AppRun"))
        .output()
        .expect("AppRun executes");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
}
