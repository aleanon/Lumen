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

// --- E.1: SBOM + AppImage ------------------------------------------------------

/// One SBOM entry (name, version, license) from `cargo metadata`.
#[derive(Clone, Debug, Serialize)]
pub struct SbomPackage {
    /// Crate name.
    pub name: String,
    /// Version.
    pub version: String,
    /// SPDX license expression (empty if undeclared).
    pub license: String,
}

/// Generate a dependency SBOM for the crate in `dir` via `cargo metadata`
/// (dependency-free, deterministic). Complementary to `cargo auditable`,
/// which embeds the same list INSIDE the binary when installed — `package`
/// uses it automatically for the release build.
pub fn sbom(dir: &Path) -> std::io::Result<Vec<SbomPackage>> {
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(dir)
        .output()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut pkgs: Vec<SbomPackage> = v["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| SbomPackage {
            name: p["name"].as_str().unwrap_or("").to_string(),
            version: p["version"].as_str().unwrap_or("").to_string(),
            license: p["license"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    pkgs.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    pkgs.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    Ok(pkgs)
}

/// Assemble an AppDir (the standard AppImage payload layout) from a bundle:
/// `AppRun` → the entry, a `.desktop` file, and a generated icon. Returns the
/// AppDir path.
pub fn build_appdir(bundle: &Path, name: &str) -> std::io::Result<PathBuf> {
    let appdir = bundle.with_extension("AppDir");
    let bin_dir = appdir.join("usr/bin");
    std::fs::create_dir_all(&bin_dir)?;
    // Copy the bundle contents under usr/bin so relative assets resolve.
    for entry in std::fs::read_dir(bundle)? {
        let entry = entry?;
        let to = bin_dir.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    // AppRun: exec the entry from usr/bin.
    let apprun = appdir.join("AppRun");
    std::fs::write(
        &apprun,
        format!("#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\nexec \"$HERE/usr/bin/{name}\" \"$@\"\n"),
    )?;
    make_executable(&apprun)?;
    make_executable(&bin_dir.join(name))?;
    std::fs::write(
        appdir.join(format!("{name}.desktop")),
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={name}\nIcon={name}\nCategories=Utility;\n"
        ),
    )?;
    // A generated 16×16 solid-accent PNG icon (no asset shipping).
    std::fs::write(appdir.join(format!("{name}.png")), icon_png())?;
    Ok(appdir)
}

/// Turn an AppDir into a runnable `.AppImage`: squashfs the AppDir
/// (`mksquashfs`) and prepend the type-2 runtime. The runtime blob is cached
/// at `~/.cache/lumen/runtime-<arch>`; when absent and not downloadable the
/// AppDir itself is the (documented) degraded artifact. Returns the AppImage
/// path.
pub fn build_appimage(appdir: &Path, name: &str) -> std::io::Result<PathBuf> {
    let sqfs = appdir.with_extension("squashfs");
    let status = std::process::Command::new("mksquashfs")
        .arg(appdir)
        .arg(&sqfs)
        .args(["-root-owned", "-noappend", "-quiet"])
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("mksquashfs failed"));
    }
    let runtime = runtime_blob()?;
    let out = appdir.with_file_name(format!("{name}.AppImage"));
    let mut bytes = runtime;
    bytes.extend(std::fs::read(&sqfs)?);
    std::fs::write(&out, bytes)?;
    make_executable(&out)?;
    let _ = std::fs::remove_file(&sqfs);
    Ok(out)
}

/// The cached type-2 AppImage runtime for this arch, downloading once via
/// `curl` when missing (dev-box convenience; CI caches the file).
fn runtime_blob() -> std::io::Result<Vec<u8>> {
    let arch = std::env::consts::ARCH;
    let cache = dirs_cache().join(format!("runtime-{arch}"));
    if let Ok(b) = std::fs::read(&cache) {
        if !b.is_empty() {
            return Ok(b);
        }
    }
    std::fs::create_dir_all(cache.parent().unwrap())?;
    let url = format!(
        "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-{arch}"
    );
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&cache)
        .arg(&url)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other(
            "AppImage runtime unavailable (offline?) — the .AppDir is the artifact; \
             cache the type2-runtime at ~/.cache/lumen/ to produce .AppImage",
        ));
    }
    std::fs::read(&cache)
}

fn dirs_cache() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        })
        .join("lumen")
}

fn make_executable(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(p)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(p, perm)?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dst)?;
        } else {
            std::fs::copy(entry.path(), &dst)?;
        }
    }
    Ok(())
}

/// A tiny generated PNG icon (16×16 Lumen-blue square).
fn icon_png() -> Vec<u8> {
    // Pre-encoded via tiny-skia once; embedding the bytes keeps lumen-cli
    // free of an image encoder. 16×16 RGBA solid #2b6cff.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0xF3, 0xFF, 0x61, 0x00, 0x00, 0x00, 0x1D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xD4,
        0xCE, 0xF9, 0xFF, 0x9F, 0x81, 0x02, 0xC0, 0x44, 0x89, 0xE6, 0x51, 0x03, 0x46, 0x0D, 0x18,
        0x35, 0x60, 0x30, 0x19, 0x00, 0x00, 0xBA, 0xFA, 0x02, 0xB5, 0xEB, 0x00, 0x9C, 0x51, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    PNG.to_vec()
}
