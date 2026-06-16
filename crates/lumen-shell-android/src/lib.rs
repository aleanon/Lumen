//! Android shell (T3.1): runs a Lumen app on a `NativeActivity`, software-
//! blitting the CPU reference frame to the `ANativeWindow` and feeding touch
//! input through the one input queue.
//!
//! All Android-specific code is gated to `target_os = "android"`; on the host
//! this crate is empty so the desktop workspace still builds and lints cleanly.

#![warn(missing_docs)]

#[cfg(target_os = "android")]
mod imp;

#[cfg(target_os = "android")]
pub use imp::run;
