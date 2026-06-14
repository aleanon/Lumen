//! Identity.
//!
//! Two distinct notions (02 §2):
//! - [`NodeIndex`] — dense, generational *runtime* identity of a live node;
//!   indexes the SoA hot-data arrays directly. Reused after removal.
//! - [`StableId`] — author-assigned identity, stable across rebuilds, reloads,
//!   and sessions. Keys state, test locators, and the agent protocol.

use smol_str::SmolStr;

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
