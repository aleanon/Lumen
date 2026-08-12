//! Run a Lumen app's background work on a real async runtime.
//!
//! # Why this crate exists
//!
//! [`lumen_core::tasks::Spawner`] is the seam, and it already has four
//! implementations — but the one meant for real work,
//! [`ThreadPoolSpawner`](lumen_core::tasks::ThreadPoolSpawner), does this:
//!
//! ```ignore
//! fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
//!     self.queue(Box::new(move || block_on(fut)))
//! }
//! ```
//!
//! It **blocks a pool thread per future**. Three consequences:
//!
//! 1. Concurrency is capped at the pool size — four by default (CACHE1), so
//!    four in-flight awaits and the fifth waits however idle they are.
//! 2. A future that needs a reactor — a `tokio::time::sleep`, a tokio socket,
//!    anything from that ecosystem — does not merely run slowly, it **panics**:
//!    there is no runtime in the thread's context. `tests/timer.rs` is that
//!    claim, executable.
//! 3. Cancellation is cooperative only. The pool's flag can drop a job that has
//!    not started and can do nothing once a worker is inside it.
//!
//! An app that brings its own HTTP client brings that client's runtime
//! requirements with it. These adapters are how it does that.
//!
//! # Not a transport
//!
//! Deliberately no HTTP, and no plans for it: what you fetch with is yours to
//! choose. This crate only makes your futures runnable.
//!
//! # Use
//!
//! ```ignore
//! use lumen_exec::TokioSpawner;
//!
//! let rt = tokio::runtime::Runtime::new().unwrap();
//! App::new(build).with_executor(TokioSpawner::from_handle(rt.handle().clone()))
//! ```
//!
//! Both adapters are native-only: [`BoxFuture`](lumen_core::tasks::BoxFuture)
//! is `!Send` on wasm, where
//! [`WasmSpawner`](lumen_core::tasks::WasmSpawner) already owns the platform.
//!
//! # Tests keep the deterministic spawners
//!
//! `InlineSpawner` / `ManualSpawner` are what golden and coherence tests use,
//! and nothing here changes that. These are for apps.
#![warn(missing_docs)]

#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
mod tokio_spawner;
#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
pub use tokio_spawner::TokioSpawner;

#[cfg(all(feature = "smol", not(target_arch = "wasm32")))]
mod smol_spawner;
#[cfg(all(feature = "smol", not(target_arch = "wasm32")))]
pub use smol_spawner::SmolSpawner;
