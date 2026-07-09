//! Android shell (T3.1 ◐): runs a Lumen app on a `NativeActivity`,
//! software-blitting the CPU reference frame to the `ANativeWindow`.
//! **Input is not wired yet**: touch/key events are currently dropped in
//! `imp.rs` (no touch, IME, safe-area insets, or back-button handling) —
//! see `docs/plan-remediation-2026-07.md` task P.1. Tier-1 `.lss` hot
//! reload over the adb-forwarded socket works.
//!
//! All Android-specific code is gated to `target_os = "android"`; on the host
//! this crate is empty so the desktop workspace still builds and lints cleanly.

#![warn(missing_docs)]

#[cfg(target_os = "android")]
mod imp;

#[cfg(target_os = "android")]
pub use imp::{run, run_styled};
