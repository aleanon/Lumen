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
