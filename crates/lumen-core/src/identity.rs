//! Identity.
//!
//! Three distinct notions (02 §2, ADR-021):
//! - [`NodeIndex`] — dense, generational *runtime* identity of a live node;
//!   indexes the SoA hot-data arrays directly. Reused after removal.
//! - [`StableId`] — author-assigned identity, stable across rebuilds, reloads,
//!   and sessions. Keys state, test locators, and the agent protocol.
//! - [`IdHash`] — *reactive* identity: the hash a signal/scope key folds to
//!   ([`hash_id`] / [`fold_id`]). Lets `cx.signal`/`cx.scope` take any
//!   `Hash` key (enum variant, index, tuple, `&str`) and address it without
//!   allocating a string every frame.

use smol_str::SmolStr;
use std::hash::{Hash, Hasher};

/// Reactive identity: the folded hash of a signal/scope key (ADR-021).
///
/// 128 bits deliberately. A collision would silently alias two signals — and
/// corrupt a state snapshot, since [`crate::state::StateSnapshot`] is keyed by
/// the *name* recorded for an id. At 128 bits that is not a practical concern
/// (~2⁻¹²⁸ per pair), so identity needs no collision probe or diagnostic.
pub type IdHash = u128;

/// The identity of the root (unscoped) namespace — what a key folds against
/// when no [`crate::state::Runtime`] scope encloses it.
pub const ROOT_ID: IdHash = 0;

// FxHash-style odd multipliers. Two independent lanes give the 128-bit width.
const K1: u64 = 0x517c_c1b7_2722_0a95;
const K2: u64 = 0x9e37_79b9_7f4a_7c15;

/// The hasher behind [`IdHash`] — a two-lane FxHash.
///
/// Hand-rolled on purpose (ADR-021): `DefaultHasher` (SipHash) is documented as
/// **not stable across Rust releases**, and reactive identity feeds persisted
/// snapshots and goldens, so the algorithm must be pinned by our own source.
/// Seed-free and endian-independent — every integer width is mixed directly and
/// byte runs are read little-endian — so the same key yields the same id on
/// every platform and every run.
///
/// Not a general-purpose or DoS-resistant hasher; it exists only to give keys a
/// stable identity.
pub struct IdHasher {
    a: u64,
    b: u64,
}

impl Default for IdHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl IdHasher {
    /// A fresh hasher.
    pub const fn new() -> IdHasher {
        IdHasher { a: 0, b: K2 }
    }

    #[inline]
    fn mix(&mut self, w: u64) {
        self.a = (self.a.rotate_left(5) ^ w).wrapping_mul(K1);
        self.b = (self.b.rotate_left(17) ^ w).wrapping_mul(K2);
    }

    /// Mix an integer, tagged by its width.
    ///
    /// `Hash` hands every integer width the same byte stream for the same
    /// numeric value, so without a tag `1u32` and `1u64` would address the
    /// *same* signal. The tag costs nothing and removes that aliasing.
    #[inline]
    fn mix_int(&mut self, v: u64, width_tag: u64) {
        self.mix(v ^ width_tag.wrapping_mul(K2));
    }

    /// The full 128-bit identity accumulated so far.
    pub fn finish128(&self) -> IdHash {
        ((self.b as u128) << 64) | self.a as u128
    }
}

impl Hasher for IdHasher {
    /// The low lane. [`IdHasher::finish128`] is what reactive identity uses.
    fn finish(&self) -> u64 {
        self.a
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            // Little-endian explicitly: `to_ne_bytes` would make ids differ
            // between a big-endian host and the snapshots/goldens it reads.
            self.mix(u64::from_le_bytes(c.try_into().unwrap()));
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rem.len()].copy_from_slice(rem);
            self.mix(u64::from_le_bytes(buf));
        }
        // Frame by length so ("ab","c") and ("a","bc") can't fold alike.
        self.mix(bytes.len() as u64);
    }

    // Mix integers directly — the default forwards to `write(&to_ne_bytes())`,
    // which is endian-dependent. Each width carries its own tag (see `mix_int`).
    fn write_u8(&mut self, i: u8) {
        self.mix_int(i as u64, 1)
    }
    fn write_u16(&mut self, i: u16) {
        self.mix_int(i as u64, 2)
    }
    fn write_u32(&mut self, i: u32) {
        self.mix_int(i as u64, 3)
    }
    fn write_u64(&mut self, i: u64) {
        self.mix_int(i, 4)
    }
    fn write_usize(&mut self, i: usize) {
        self.mix_int(i as u64, 5)
    }
    fn write_u128(&mut self, i: u128) {
        self.mix_int(i as u64, 6);
        self.mix_int((i >> 64) as u64, 7)
    }
    fn write_i8(&mut self, i: i8) {
        self.mix_int(i as u64, 8)
    }
    fn write_i16(&mut self, i: i16) {
        self.mix_int(i as u64, 9)
    }
    fn write_i32(&mut self, i: i32) {
        self.mix_int(i as u64, 10)
    }
    fn write_i64(&mut self, i: i64) {
        self.mix_int(i as u64, 11)
    }
    fn write_isize(&mut self, i: isize) {
        self.mix_int(i as u64, 12)
    }
    fn write_i128(&mut self, i: i128) {
        self.mix_int(i as u64, 13);
        self.mix_int((i >> 64) as u64, 14)
    }
}

/// The identity of a bare key, before any scope is folded in.
///
/// ```
/// use lumen_core::identity::hash_id;
/// #[derive(Hash)]
/// enum Field { Row(u32) }
/// assert_eq!(hash_id(&Field::Row(3)), hash_id(&Field::Row(3)));
/// assert_ne!(hash_id(&Field::Row(3)), hash_id(&Field::Row(4)));
/// assert_eq!(hash_id("filter"), hash_id("filter"));
/// ```
pub fn hash_id<K: Hash + ?Sized>(key: &K) -> IdHash {
    let mut h = IdHasher::new();
    key.hash(&mut h);
    h.finish128()
}

/// Fold a local key's hash into its enclosing scope's identity.
///
/// This is what makes identity hierarchical without building a `"parent/child"`
/// string: a scope hands its children its own [`IdHash`], and each child folds
/// its local key into it. Order matters — `fold_id(a, b) != fold_id(b, a)`.
///
/// ```
/// use lumen_core::identity::{fold_id, hash_id, ROOT_ID};
/// let row = fold_id(ROOT_ID, hash_id("row-1"));
/// let text_in_row = fold_id(row, hash_id("text"));
/// // The same local key under a different scope is a different signal.
/// let other = fold_id(fold_id(ROOT_ID, hash_id("row-2")), hash_id("text"));
/// assert_ne!(text_in_row, other);
/// ```
pub fn fold_id(parent: IdHash, local: IdHash) -> IdHash {
    let (pa, pb) = (parent as u64, (parent >> 64) as u64);
    let (la, lb) = (local as u64, (local >> 64) as u64);
    let a = (pa.rotate_left(5) ^ la).wrapping_mul(K1);
    let b = (pb.rotate_left(17) ^ lb).wrapping_mul(K2);
    ((b as u128) << 64) | a as u128
}

/// Runtime identity of a live node: a slot `index` plus a `generation` stamp.
///
/// Dense and reused after removal. The generation is bumped each time a slot is
/// recycled, so a `NodeIndex` captured before its node was removed will fail the
/// tree's liveness check rather than alias a different node.
///
/// `NodeIndex::NONE` is the null sentinel used by the intrusive tree links.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIndex {
    index: u32,
    generation: u32,
}

impl NodeIndex {
    /// The null node: used for absent parents/children/siblings in the tree
    /// link arrays, and as the value of an empty tree's root.
    pub const NONE: NodeIndex = NodeIndex {
        index: u32::MAX,
        generation: u32::MAX,
    };

    /// Construct a node index. Crate-internal: only the tree allocator mints
    /// these so the `(index, generation)` pairing stays authoritative.
    pub(crate) const fn new(index: u32, generation: u32) -> NodeIndex {
        NodeIndex { index, generation }
    }

    /// The dense slot index. Use to address SoA arrays. Meaningless for
    /// [`NodeIndex::NONE`].
    pub fn index(self) -> u32 {
        self.index
    }

    /// The generation stamp at the time this index was minted.
    pub fn generation(self) -> u32 {
        self.generation
    }

    /// True if this is the null sentinel [`NodeIndex::NONE`].
    pub fn is_none(self) -> bool {
        self.index == u32::MAX
    }

    /// True if this refers to a (possibly stale) real slot.
    pub fn is_some(self) -> bool {
        !self.is_none()
    }
}

impl std::fmt::Debug for NodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            f.write_str("NodeIndex::NONE")
        } else {
            write!(f, "NodeIndex({}v{})", self.index, self.generation)
        }
    }
}

/// Author-assigned identity, stable across rebuilds, reloads, and sessions.
///
/// Set in widget code via `.id("save-button")`. Must be unique within its
/// window; duplicates are a runtime diagnostic ([`crate::codes::W0001`]) and the
/// first match wins for selectors (02 §2).
///
/// ```
/// use lumen_core::StableId;
/// let a = StableId::from("save-button");
/// let b: StableId = "save-button".into();
/// assert_eq!(a, b);
/// assert_eq!(a.as_str(), "save-button");
/// ```
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StableId(pub SmolStr);

impl StableId {
    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for StableId {
    fn from(s: &str) -> Self {
        StableId(SmolStr::new(s))
    }
}

impl From<String> for StableId {
    fn from(s: String) -> Self {
        StableId(SmolStr::new(s))
    }
}

impl From<SmolStr> for StableId {
    fn from(s: SmolStr) -> Self {
        StableId(s)
    }
}

impl std::fmt::Display for StableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Debug for StableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StableId({:?})", self.as_str())
    }
}

#[cfg(test)]
mod id_hash_tests {
    use super::*;
    use std::collections::HashSet;

    /// Reactive identity feeds persisted snapshots (ADR-011) and goldens, so the
    /// hash must never drift silently: changing [`IdHasher`] would re-key every
    /// stored value and orphan every snapshot. These literals are the tripwire —
    /// if they fail, the change was deliberate *or* it is a real regression, and
    /// either way it has to be acknowledged.
    #[test]
    fn identity_is_stable_across_runs_and_builds() {
        assert_eq!(hash_id("save-button"), GOLDEN_SAVE_BUTTON);
        assert_eq!(hash_id(&7u32), GOLDEN_U32_7);
        assert_eq!(fold_id(ROOT_ID, hash_id("row-1")), GOLDEN_ROOT_FOLDED_ROW_1);
    }

    const GOLDEN_SAVE_BUTTON: IdHash = 0x23b9_2a22_b807_8382_99b2_530b_bf15_dbec;
    const GOLDEN_U32_7: IdHash = 0x85fb_53aa_e2ef_970e_7eb1_5639_f508_d498;
    const GOLDEN_ROOT_FOLDED_ROW_1: IdHash = 0x0a39_df0b_807b_d593_8c5d_fd4a_1dfb_abce;

    #[test]
    fn same_key_hashes_the_same_every_time() {
        assert_eq!(hash_id("filter"), hash_id("filter"));
        assert_eq!(hash_id(&(3u32, "x")), hash_id(&(3u32, "x")));
    }

    /// The realistic key shapes must not alias each other.
    #[test]
    fn distinct_keys_get_distinct_ids() {
        #[derive(Hash)]
        enum Field {
            Filter,
            Row(u32),
            Cell(u32, u32),
        }
        let mut seen = HashSet::new();
        let mut push = |h: IdHash, what: &str| {
            assert!(seen.insert(h), "identity collision for {what}");
        };
        push(hash_id(&Field::Filter), "Field::Filter");
        for i in 0..1_000u32 {
            push(hash_id(&Field::Row(i)), "Field::Row(i)");
            push(hash_id(&i), "bare u32");
            push(hash_id(&format!("row-{i}")), "string key");
        }
        for r in 0..30u32 {
            for c in 0..30u32 {
                push(hash_id(&Field::Cell(r, c)), "Field::Cell(r,c)");
            }
        }
    }

    /// `Hash` gives every integer width the same byte stream for one numeric
    /// value; the width tag is what stops `1u32` and `1u64` sharing a signal.
    #[test]
    fn integer_widths_do_not_alias() {
        assert_ne!(hash_id(&1u32), hash_id(&1u64));
        assert_ne!(hash_id(&1u8), hash_id(&1usize));
        assert_ne!(hash_id(&1i32), hash_id(&1u32));
    }

    /// Concatenation must not be ambiguous — the length framing in `write`.
    #[test]
    fn adjacent_string_fields_cannot_realign() {
        assert_ne!(hash_id(&("ab", "c")), hash_id(&("a", "bc")));
    }

    #[test]
    fn folding_is_order_sensitive_and_hierarchical() {
        let a = hash_id("a");
        let b = hash_id("b");
        assert_ne!(fold_id(a, b), fold_id(b, a), "fold must not be commutative");

        // The same local key under two different scopes is two signals.
        let row1 = fold_id(ROOT_ID, hash_id("row-1"));
        let row2 = fold_id(ROOT_ID, hash_id("row-2"));
        assert_ne!(
            fold_id(row1, hash_id("text")),
            fold_id(row2, hash_id("text"))
        );

        // ...and nesting is not flattenable to a single fold.
        assert_ne!(
            fold_id(row1, hash_id("text")),
            fold_id(ROOT_ID, hash_id("row-1text"))
        );
    }

    /// A deep scope chain must not converge on a fixed point (which would make
    /// every deeply-nested signal the same one).
    #[test]
    fn deep_nesting_keeps_producing_new_ids() {
        let mut seen = HashSet::new();
        let mut h = ROOT_ID;
        for depth in 0..500u32 {
            h = fold_id(h, hash_id(&depth));
            assert!(
                seen.insert(h),
                "fold reached a fixed point at depth {depth}"
            );
        }
    }
}
