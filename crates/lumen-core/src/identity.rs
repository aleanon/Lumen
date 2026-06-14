//! Author-assigned identity.
//!
//! [`StableId`] is the identity that survives rebuilds, hot reloads, and
//! sessions (02 §2). It keys state, test locators, and the agent protocol.
//! Runtime identity (`NodeIndex`, dense + generational) is a separate concept
//! that arrives with the node tree in T0.2.

use smol_str::SmolStr;

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
