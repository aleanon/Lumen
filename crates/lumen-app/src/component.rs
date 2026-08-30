//! [`Component`] — a screen or section as a type, and the unit of rebuild.
//!
//! ## Why this exists
//!
//! Measured (R7, `benches/src/bin/sparse.rs`): at 50 000 rows with **one** row
//! changing, a `cx.scope` per row costs **42.9 ms**; the same rows grouped 256
//! to a scope cost **9.2 ms**. The 4.7× is not a framework optimisation — both
//! runs use the same engine. It is the *shape* the author wrote.
//!
//! The breakdown of that 4.7×:
//!
//! | cost | per-row scope | grouped |
//! |---|---:|---:|
//! | view (`scope_with_deps` @ ~0.28 µs/call) | 16 604 µs | 2 736 µs |
//! | layout (root flex re-solves across its children) | 13 258 µs | 2 724 µs |
//! | `sweep_dead_scopes`, O(scopes) | 752 µs | 3 µs |
//!
//! Three of those are the author's choice of granularity, and until now the
//! framework offered no construct that steered anyone toward the good one.
//! `cx.scope` is *available* at any granularity, which means per-row — the
//! worst one — is the obvious way to reach for it.
//!
//! A component makes the coarse grain the natural unit: you write a screen or a
//! section as a struct, and it becomes one memoized subtree. Nesting comes free
//! with it, which is what fixes the layout half (a flat container re-solves
//! across all N children; a nested one does not — R6).
//!
//! ## What it is
//!
//! A plain struct plus a `build`. Construct it, mutate it, then hand it over:
//!
//! ```ignore
//! struct Header { title: String, unread: u32 }
//!
//! impl Component for Header {
//!     fn deps(&self) -> u64 { hash_of(&(&self.title, self.unread)) }
//!     fn build(&self, cx: &mut BuildCx) -> Element {
//!         widgets::column(vec![
//!             widgets::text(self.title.clone()),
//!             widgets::text(format!("{} unread", self.unread)),
//!         ])
//!     }
//! }
//!
//! let mut h = Header { title: "Inbox".into(), unread: 0 };
//! h.unread = count;                 // mutate the struct freely
//! cx.component("header", h)         // …then hand it over
//! ```
//!
//! `build` runs only when `deps` changes or when a signal it read changes.
//! Otherwise the whole subtree — nodes, layout and all — is spliced in place.
//!
//! ## Rebuild is teardown-and-rewrite
//!
//! When a component *is* dirty its subtree is rebuilt from scratch rather than
//! diffed. That is the right trade precisely because a component is coarse: the
//! cost is bounded by one component, and avoiding a reconciler keeps the engine
//! free of a second source of truth about what the tree contains.
//!
//! Scope identity survives a rebuild, so scope-local signals and running tasks
//! are *not* lost when `deps` changes — only the nodes are.

use crate::element::{BuildCx, Element};
use lumen_core::identity::hash_id;
use std::fmt::Debug;
use std::hash::Hash;

/// Hash any plain data into the form [`Component::deps`] returns.
pub fn hash_of<T: Hash + ?Sized>(v: &T) -> u64 {
    // Fold the 128-bit structural hash down; `deps` only needs to detect
    // change, not to address anything.
    let h = hash_id(v);
    (h as u64) ^ ((h >> 64) as u64)
}

/// Returned from [`Component::deps`] by a component that holds no plain data —
/// everything it renders comes from signals read inside `build`, which the
/// engine tracks on its own.
pub const SIGNALS_ONLY: u64 = 0;

/// A screen or section: a struct that knows how to build itself, and is the
/// unit the engine rebuilds.
///
/// See the [module docs](self) for why the granularity matters.
pub trait Component {
    /// A hash of every piece of **plain data** this component's output depends
    /// on — the values it captured rather than read from a signal.
    ///
    /// Signals read inside [`build`](Self::build) are tracked automatically and
    /// must **not** be listed here.
    ///
    /// There is deliberately no default. A component built from captured data
    /// whose `deps` omitted that data would be memo-hit forever and render
    /// frozen content — silently, with no panic and no diagnostic. Making this
    /// required costs one line and removes the failure mode. Return
    /// [`SIGNALS_ONLY`] when there genuinely is no plain data.
    fn deps(&self) -> u64;

    /// Build this component's subtree.
    ///
    /// Runs only when [`deps`](Self::deps) changed or a signal it read changed.
    fn build(&self, cx: &mut BuildCx) -> Element;
}

impl BuildCx<'_> {
    /// Mount a [`Component`] as one memoized subtree.
    ///
    /// `key` distinguishes siblings and must be stable across builds — the same
    /// contract as [`scope`](BuildCx::scope). In a list, key by the item's
    /// identity, never by its position, or reordering re-renders everything and
    /// sheds each item's scope-local state.
    pub fn component<K: Hash + Debug, C: Component>(&mut self, key: K, c: C) -> Element {
        let deps = c.deps();
        self.scope_with_deps(key, deps, move |cx| c.build(cx))
    }
}
