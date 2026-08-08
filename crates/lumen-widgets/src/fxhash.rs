//! CP3.2: a fast, non-cryptographic hasher for the rebuild path's hot maps.
//!
//! The rebuild path is dominated by `HashMap` traffic keyed on `NodeIndex`,
//! `IdHash` and short strings — none of it adversarial, all of it re-done every
//! frame. std's default `SipHash-1-3` is DoS-resistant, which is the wrong
//! trade for keys the app itself mints. This is the FxHash construction
//! (rustc's own), which is a multiply and a rotate per word.
//!
//! It does NOT touch `lumen_core::IdHasher`,
//! which ADR-021 pins because snapshots and goldens depend on its exact
//! output — this one only affects bucket order inside maps, never a serialized
//! value. CP3.1 had to land first for that to be true: `what_depends_on`
//! serialized a `Vec` built in `HashMap` iteration order, so changing the
//! hasher would have silently permuted agent-visible output.

#[derive(Default, Clone, Copy)]
pub(crate) struct FxHasher {
    hash: u64,
}

impl FxHasher {
    /// The odd multiplier from rustc's FxHash (φ⁻¹ scaled to 64 bits).
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add_to_hash(&mut self, w: u64) {
        self.hash = (self.hash.rotate_left(5) ^ w).wrapping_mul(Self::SEED);
    }
}

impl std::hash::Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Word-at-a-time over the body, then the tail. Short keys (the common
        // case here — node indices and small ids) take one or two rounds.
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.add_to_hash(u64::from_ne_bytes(c.try_into().unwrap()));
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rem.len()].copy_from_slice(rem);
            self.add_to_hash(u64::from_ne_bytes(buf));
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_to_hash(u64::from(i));
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add_to_hash(u64::from(i));
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_to_hash(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_to_hash(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub(crate) type FxBuildHasher = std::hash::BuildHasherDefault<FxHasher>;

/// `std::collections::HashMap` with the module's fast hasher.
///
/// Same API except `new()`, which std only provides for `RandomState`; use
/// `default()`. Shadowing the std name keeps the ~35 declarations in this file
/// unchanged.
pub(crate) type HashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;
