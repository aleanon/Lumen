//! Tier-2 hot patch (ADR-012): load component cdylibs at runtime and swap them
//! in place without restarting the process.
//!
//! State lives in the host-owned [`lumen_core::Runtime`], never inside the
//! dylib, so reloading the component leaves application state untouched. Each
//! superseded library is *retired* (kept alive, never `dlclose`d): live
//! pointers may still reference its code/rodata, so unloading would risk a
//! use-after-free.
//!
//! # What this module actually is — and is not
//!
//! **Tier 2 is off by default and must be opted into with `LUMEN_TIER2=1`.**
//! Without it, [`HotComponent::swap`] always reports [`Tier3Reason::NotEnabled`]
//! and the caller performs a snapshot restart. That default is deliberate.
//!
//! What is verified here is the *protocol*: that a `libloading` swap is fast,
//! that retiring the old library is safe, that host-owned state survives, and
//! that an incompatible candidate cleanly downgrades to tier 3. That is real
//! and tested.
//!
//! What is **not** verified is ABI compatibility. [`HOST_ABI_HASH`] is a fixed
//! placeholder that fingerprints nothing — not the compiler version, not the
//! core crates, not the layout of any type crossing the boundary. Rust has no
//! stable ABI and `repr(Rust)` layout may differ between two builds of the same
//! source, so a genuine fingerprint is a research problem, not an oversight
//! here. Until one exists, an "ABI match" means only "both sides quote the same
//! constant", which is why a match alone is not allowed to authorize a swap.
//!
//! Treating a placeholder as a safety gate is worse than having no gate: it
//! reads as verified in review and in the docs, and it would happily load a
//! mismatched library and hand you memory corruption instead of a clean error.
//! Hence opt-in.
//!
//! Note also that the fixtures (`crates/fixtures/hot_{a,b,c}`) exercise only two
//! `extern "C"` symbols, one of which returns a static string — no
//! `Element`-building code crosses the FFI boundary today.

use libloading::{Library, Symbol};
use std::ffi::{c_char, CStr};
use std::path::Path;

/// Placeholder ABI token. **This fingerprints nothing** — see the module docs.
///
/// It is compared against the candidate's `lumen_abi_hash` purely to exercise
/// the mismatch → tier-3 downgrade path. A real fingerprint would have to cover
/// the compiler version and the layout of every type crossing the boundary;
/// Rust offers no stable ABI to derive one from.
pub const HOST_ABI_HASH: u64 = 0x1111_2222_3333_4444;

/// Environment variable that opts into the tier-2 in-place swap.
pub const TIER2_ENV: &str = "LUMEN_TIER2";

/// Whether the tier-2 swap path is enabled (`LUMEN_TIER2=1`).
///
/// Defaults to `false`: the ABI check cannot establish compatibility (see the
/// module docs), so in-place patching is opt-in and the safe tier-3 snapshot
/// restart is what runs unless a developer explicitly asks otherwise.
pub fn tier2_enabled() -> bool {
    std::env::var(TIER2_ENV).is_ok_and(|v| v == "1")
}

type AbiHashFn = unsafe extern "C" fn() -> u64;
type BuildLabelFn = unsafe extern "C" fn() -> *const c_char;

/// Why a candidate could not be swapped in place.
#[derive(Debug, PartialEq, Eq)]
pub enum Tier3Reason {
    /// Tier 2 was not opted into — the default. Set `LUMEN_TIER2=1` to enable
    /// in-place swapping, understanding that the ABI check cannot establish
    /// compatibility (see the module docs).
    NotEnabled,
    /// The candidate quoted a different ABI token than the host.
    ///
    /// Note this proves *incompatibility* but a match does not prove
    /// compatibility — [`HOST_ABI_HASH`] is a placeholder.
    AbiMismatch {
        /// ABI token the host quotes.
        host: u64,
        /// ABI token the candidate cdylib reported.
        found: u64,
    },
}

/// Outcome of an attempted swap.
#[derive(Debug, PartialEq, Eq)]
pub enum Swap {
    /// Hot-swapped in place (tier 2); carries the new build() output.
    Patched(String),
    /// Not swapped: the caller must restart via tier-3 snapshot restore.
    NeedsTier3(Tier3Reason),
}

/// A loaded component cdylib plus the libraries it has superseded.
pub struct HotComponent {
    current: Library,
    retired: Vec<Library>,
    label: String,
    /// Captured from [`tier2_enabled`] at load. Held per-component rather than
    /// read from the environment on each swap so that callers (and tests) can
    /// set it explicitly without mutating process-wide state.
    tier2: bool,
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
            tier2: tier2_enabled(),
        })
    }

    /// Override the tier-2 opt-in for this component.
    ///
    /// Defaults to [`tier2_enabled`] (i.e. off unless `LUMEN_TIER2=1`). Enabling
    /// it accepts that the ABI token cannot establish compatibility — see the
    /// module docs.
    pub fn set_tier2(&mut self, enabled: bool) {
        self.tier2 = enabled;
    }

    /// Whether this component will attempt an in-place swap.
    pub fn tier2_enabled(&self) -> bool {
        self.tier2
    }

    /// The component's current `build()` output.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Number of retired (intentionally leaked) libraries — one per swap.
    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }

    /// Attempt to hot-swap to a freshly built cdylib.
    ///
    /// Returns [`Swap::NeedsTier3`] unless tier 2 has been opted into via
    /// `LUMEN_TIER2=1` **and** the candidate quotes the same ABI token. On any
    /// tier-3 outcome the current library is left untouched.
    pub fn swap(&mut self, path: &Path) -> Result<Swap, String> {
        // Checked before loading anything: with tier 2 off there is no reason
        // to map a dylib we are not going to adopt.
        if !self.tier2 {
            return Ok(Swap::NeedsTier3(Tier3Reason::NotEnabled));
        }
        // SAFETY: same contract as `load`.
        let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
        let abi = read_abi(&lib)?;
        if abi != HOST_ABI_HASH {
            return Ok(Swap::NeedsTier3(Tier3Reason::AbiMismatch {
                host: HOST_ABI_HASH,
                found: abi,
            }));
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
