//! MOD4: third-party `.lss` properties.
//!
//! `Style::apply` is a closed `match` over a fixed property list, so before
//! this there was no way to add a style property without forking the crate —
//! the one clearly-absent extension point against the architecture doc's claim
//! that third-party widgets and styling are first-class.
//!
//! # What a registered property does, and does not
//!
//! Registering a name does two things: the parser stops reporting `E0102`
//! ("unknown property") for it, and its parsed [`Value`] is carried on
//! [`Style::custom`] through the cascade, so whoever registered it can read the
//! resolved value per node.
//!
//! It deliberately does **not** let a third party mutate `Style`'s built-in
//! fields. Those are the contract the layout and render pipelines consume, and
//! an extension that could rewrite them would be able to break invariants the
//! framework asserts elsewhere. Carrying the value is the useful half:
//! a custom leaf or renderer reads it and decides what it means.
//!
//! # Cost
//!
//! Registration is process-global and expected once at startup. The cascade
//! only touches `custom` for properties that were actually registered *and*
//! actually used, so an app that registers nothing pays one empty-set check per
//! resolved declaration — and `Style::apply` itself runs behind the style memo,
//! not per node per frame.

use std::collections::BTreeSet;
use std::sync::RwLock;

/// Process-global set of extra property names.
///
/// `BTreeSet` rather than a hash set so iteration order is deterministic:
/// `computed_json` and the agent's `ui.getStyles` serialize these, and stable
/// ordering is what keeps that output diffable.
static EXTRA: RwLock<BTreeSet<String>> = RwLock::new(BTreeSet::new());

/// Register a `.lss` property name the framework does not implement.
///
/// Idempotent. Returns `false` if the name is already a built-in property —
/// registering over one is refused rather than silently shadowing it, since a
/// third party quietly overriding `width` would be a much worse surprise than
/// a rejected call.
///
/// ```
/// # use lumen_style::{register_property, parse, has_errors};
/// assert!(register_property("elevation"));
/// let (_, diags) = parse("t.lss", "card { elevation: 3; }");
/// assert!(!has_errors(&diags), "a registered property must not be E0102");
/// ```
pub fn register_property(name: &str) -> bool {
    if crate::KNOWN_PROPERTIES.contains(&name) {
        return false;
    }
    match EXTRA.write() {
        // `insert` returns false for an already-present name, but re-registering
        // is legitimate (two extensions, or one called twice), so success here
        // means "the name is registered", not "it was new".
        Ok(mut set) => {
            set.insert(name.to_string());
            true
        }
        // A poisoned lock means some other thread panicked mid-registration.
        // Report failure rather than pretending: the caller can decide, and a
        // silent success would leave a property that never parses.
        Err(_) => false,
    }
}

/// Whether `name` has been registered by [`register_property`].
pub fn is_registered(name: &str) -> bool {
    EXTRA.read().map(|s| s.contains(name)).unwrap_or(false)
}

/// Every registered name, in sorted order.
pub fn registered_properties() -> Vec<String> {
    EXTRA
        .read()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default()
}

/// Whether anything has been registered at all — the fast path for the cascade.
pub fn any_registered() -> bool {
    EXTRA.read().map(|s| !s.is_empty()).unwrap_or(false)
}
