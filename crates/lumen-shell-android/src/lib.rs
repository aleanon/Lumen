//! Android shell (T3.1 ◐): runs a Lumen app on a `NativeActivity`,
//! software-blitting the CPU reference frame to the `ANativeWindow`.
//! Input is wired (P.1): `MotionEvent` becomes `PointerDown`/`Move`/`Up` in
//! content-logical space, `Cancel` becomes a release marked `click_count: 0`
//! (a cancellation — 02 §6), keys map through `KeyEvent` with Back → Escape,
//! and safe-area insets come from the content rect. Touch scrolling and taps
//! therefore go through the same paths the desktop shell uses. (True IME
//! commit text needs GameActivity — see `imp.rs`.) Tier-1 `.lss` hot reload
//! over the adb-forwarded socket works.
//!
//! All Android-specific code is gated to `target_os = "android"`; on the host
//! this crate is empty so the desktop workspace still builds and lints cleanly.

#![warn(missing_docs)]

#[cfg(target_os = "android")]
mod imp;

#[cfg(target_os = "android")]
pub use imp::{run, run_styled};
