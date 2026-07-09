//! `lumen-cli` library surface — the dev server + tier-1 hot reload (T1.7).
//!
//! The CLI binary (`src/main.rs`) handles `new`/`run`/`test`; this lib hosts the
//! dev-server pieces that are unit/integration tested.
#![warn(missing_docs)]

pub mod agent;
pub mod dev;
pub mod dist;
pub mod hotpatch;
pub mod proto;
