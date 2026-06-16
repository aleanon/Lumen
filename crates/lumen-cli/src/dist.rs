//! Distribution & packaging (T7.1): build a portable app **bundle** — a
//! directory with the executable, its assets, and a self-describing
//! `manifest.json` — the substrate every installer format (msix/dmg/AppImage/
//! apk/ipa) wraps. Bundle layout + manifest are deterministic and testable.
//!
//! Code signing + notarization, delta auto-update, and the OS installer formats
//! are platform work (they consume this bundle); the Android `.apk` path already
//! exists (`scripts/android_build_apk.sh`).

use serde::Serialize;
use std::path::{Path, PathBuf};

/// A self-describing app bundle manifest (`lumen-bundle/1`).
#[derive(Clone, Debug, Serialize)]
pub struct BundleManifest {
    /// Schema tag.
    pub schema: String,
    /// App name.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// Target platform (`linux`/`macos`/`windows`/`android`/`ios`/`web`).
    pub platform: String,
    /// Executable file name within the bundle.
    pub entry: String,
    /// Bundled asset file names.
    pub assets: Vec<String>,
}

impl BundleManifest {
    /// A manifest for `name`@`version` on `platform` with entry `entry`.
    pub fn new(name: &str, version: &str, platform: &str, entry: &str) -> BundleManifest {
        BundleManifest {
            schema: "lumen-bundle/1".to_string(),
            name: name.to_string(),
            version: version.to_string(),
            platform: platform.to_string(),
            entry: entry.to_string(),
            assets: Vec::new(),
        }
    }
}

/// Write a bundle to `out_dir/<name>.bundle/`: the manifest, the entry binary,
/// and each asset under `assets/`. Returns the bundle directory.
pub fn package(
    out_dir: &Path,
    mut manifest: BundleManifest,
    entry_bytes: &[u8],
    assets: &[(String, Vec<u8>)],
) -> std::io::Result<PathBuf> {
    let bundle = out_dir.join(format!("{}.bundle", manifest.name));
    std::fs::create_dir_all(bundle.join("assets"))?;
    std::fs::write(bundle.join(&manifest.entry), entry_bytes)?;
    manifest.assets = assets.iter().map(|(n, _)| n.clone()).collect();
    for (name, bytes) in assets {
        std::fs::write(bundle.join("assets").join(name), bytes)?;
    }
    std::fs::write(
        bundle.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )?;
    Ok(bundle)
}
