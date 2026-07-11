//! `#[state_registry]` runtime support (02 §4, plan W.4): stored **trait
//! objects**. `Box<dyn StoredTrait>` is forbidden in the state store unless
//! every concrete impl is registered — serialization writes a
//! `{ "type": <name>, "value": <fields> }` envelope, deserialization looks
//! the name up here and rebuilds the vtable.
//!
//! The `#[lumen_macros::state_registry]` attribute on a trait generates the
//! serde impls for its `Box<dyn Trait>` plus a `register_<trait>` function;
//! each concrete type declares its tag with [`stored_type!`](crate::stored_type)
//! and is registered at app startup (before any restore) with
//! `register_<trait>::<T>("tag")`. An unregistered tag in a snapshot fails
//! that value's deserialization, which the lenient restore path surfaces as
//! a `W0002` drop.

use std::collections::HashMap;
use std::sync::RwLock;

/// The serialize half a registered stored type provides: its registry tag
/// and its field-tagged JSON body. Implemented via [`stored_type!`].
pub trait StoredName {
    /// The stable registry tag written into snapshots.
    fn stored_name(&self) -> &'static str;
    /// The value's own fields, field-tagged (ADR-011).
    fn stored_json(&self) -> serde_json::Value;
}

/// One trait's tag → deserializer table. A static instance per
/// `#[state_registry]` trait, created by the generated `__<trait>_registry`.
pub struct DynRegistry<T: ?Sized> {
    #[allow(clippy::type_complexity)]
    map: RwLock<HashMap<&'static str, fn(&serde_json::Value) -> Option<Box<T>>>>,
}

impl<T: ?Sized> Default for DynRegistry<T> {
    fn default() -> Self {
        DynRegistry {
            map: RwLock::new(HashMap::new()),
        }
    }
}

impl<T: ?Sized> DynRegistry<T> {
    /// Register (or replace) the deserializer for `name`.
    pub fn insert(&self, name: &'static str, f: fn(&serde_json::Value) -> Option<Box<T>>) {
        self.map.write().expect("registry poisoned").insert(name, f);
    }

    /// Rebuild a value: look `name` up and run its deserializer.
    pub fn deserialize(&self, name: &str, value: &serde_json::Value) -> Option<Box<T>> {
        let f = *self.map.read().expect("registry poisoned").get(name)?;
        f(value)
    }
}

/// Implement [`StoredName`] for a concrete type with a stable tag:
/// `stored_type!(Circle as "circle");`. The type must be `Serialize`.
#[macro_export]
macro_rules! stored_type {
    ($ty:ty as $name:literal) => {
        impl $crate::registry::StoredName for $ty {
            fn stored_name(&self) -> &'static str {
                $name
            }
            fn stored_json(&self) -> serde_json::Value {
                serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
            }
        }
    };
}
