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
//! ## `deps` is derived, not declared (S2)
//!
//! `Component: Hash`, and `deps` defaults to hashing the whole component. Write
//! `#[derive(Hash)]` — std's, no Lumen macro — and the dependency is exact by
//! construction. This replaced C1's required `deps`, which existed only to stop
//! an author omitting a captured field and getting silently frozen content.
//!
//! *Consequence:* `Hash` has a generic method, so `Component` is **not
//! object-safe** — there is no `Box<dyn Component>`. This costs nothing in
//! practice: components produce `Element`, which is already the currency for
//! heterogeneous children, so a mixed list is a `Vec<Element>` as before.
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
pub trait Component: Hash {
    /// What this component's output depends on, beyond the signals its
    /// [`build`](Self::build) reads.
    ///
    /// **Defaults to hashing the whole component (S2), which is almost always
    /// what you want** — every captured field is included because the default
    /// cannot see past `Hash`, so it cannot *omit* one. C1 shipped this as a
    /// required method precisely because a hand-written `deps` that forgot a
    /// captured field would be memo-hit forever and render frozen content,
    /// silently. Deriving it from the fields removes the failure mode instead
    /// of documenting it.
    ///
    /// Signals read inside `build` are tracked separately and need not appear:
    /// `scope_impl` requires **both** `deps` unchanged *and* the recorded reads
    /// still current, so the two are additive. A component that captures
    /// nothing therefore hashes to a constant and correctly relies on its reads.
    ///
    /// Override when a field must not participate — a handler (`Rc<dyn Fn>`
    /// has no meaningful hash and does not affect rendering), or an `f64`,
    /// which is not `Hash`. Returning [`SIGNALS_ONLY`] states "no captured
    /// data" explicitly.
    fn deps(&self) -> u64 {
        hash_of(self)
    }

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
