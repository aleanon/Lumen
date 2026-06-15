//! Tier-2 hot patch (ADR-012): load component cdylibs at runtime and swap them
//! in place without restarting the process.
//!
//! State lives in the host-owned [`lumen_core::Runtime`], never inside the
//! dylib, so reloading the component leaves application state untouched. Each
//! superseded library is *retired* (kept alive, never `dlclose`d): live
//! pointers may still reference its code/rodata, so unloading would risk a
//! use-after-free. If a freshly built component reports a different ABI hash it
//! is incompatible and the caller must fall back to a tier-3 snapshot restart.

use libloading::{Library, Symbol};
use std::ffi::{c_char, CStr};
use std::path::Path;

/// ABI hash the host was built against (compiler + core-crate fingerprints).
/// A component whose `lumen_abi_hash` differs cannot be hot-swapped.
pub const HOST_ABI_HASH: u64 = 0x1111_2222_3333_4444;

type AbiHashFn = unsafe extern "C" fn() -> u64;
type BuildLabelFn = unsafe extern "C" fn() -> *const c_char;

/// Outcome of an attempted swap.
#[derive(Debug, PartialEq, Eq)]
pub enum Swap {
    /// Hot-swapped in place (tier 2); carries the new build() output.
    Patched(String),
    /// ABI-incompatible: the caller must restart via tier-3 snapshot restore.
    NeedsTier3 {
        /// ABI hash the host expects.
        host: u64,
        /// ABI hash the candidate cdylib reported.
        found: u64,
    },
}

/// A loaded component cdylib plus the libraries it has superseded.
pub struct HotComponent {
    current: Library,
    retired: Vec<Library>,
    label: String,
}

impl HotComponent {
    /// Load the initial component from a cdylib path.
    pub fn load(path: &Path) -> Result<HotComponent, String> {
        // SAFETY: loading a dylib runs its initializers; the fixture cdylibs
        // expose only plain `extern "C"` functions with no global ctors.
        let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
        let label = read_label(&lib)?;
        Ok(HotComponent {
            current: lib,
            retired: Vec::new(),
            label,
        })
    }

    /// The component's current `build()` output.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Number of retired (intentionally leaked) libraries — one per swap.
    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }

    /// Attempt to hot-swap to a freshly built cdylib. On an ABI match the new
    /// library is adopted and the old one retired; on a mismatch the current
    /// library is left untouched and the caller is told to use tier 3.
    pub fn swap(&mut self, path: &Path) -> Result<Swap, String> {
        // SAFETY: same contract as `load`.
        let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
        let abi = read_abi(&lib)?;
        if abi != HOST_ABI_HASH {
            return Ok(Swap::NeedsTier3 {
                host: HOST_ABI_HASH,
                found: abi,
            });
        }
        let label = read_label(&lib)?;
        let old = std::mem::replace(&mut self.current, lib);
        self.retired.push(old); // leak on purpose — see module docs
        self.label = label.clone();
        Ok(Swap::Patched(label))
    }
}

fn read_abi(lib: &Library) -> Result<u64, String> {
    // SAFETY: the symbol's Rust signature matches the cdylib's C ABI.
    unsafe {
        let f: Symbol<AbiHashFn> = lib.get(b"lumen_abi_hash").map_err(|e| e.to_string())?;
        Ok(f())
    }
}

fn read_label(lib: &Library) -> Result<String, String> {
    // SAFETY: signature matches; the returned pointer is a 'static CStr in the
    // dylib's rodata, copied into an owned String before the borrow ends.
    unsafe {
        let f: Symbol<BuildLabelFn> = lib.get(b"lumen_build_label").map_err(|e| e.to_string())?;
        let ptr = f();
        if ptr.is_null() {
            return Err("component returned a null label".into());
        }
        Ok(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}
