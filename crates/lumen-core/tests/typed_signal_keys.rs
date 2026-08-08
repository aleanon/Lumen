//! SD6b: a `SignalKey<T>` binds a key's value type at declaration.
//!
//! Signal keys are strings, so two call sites can name the same key with
//! different types. SD6a made that failure loud — the panic names the key and
//! both types — but loud-at-runtime is still runtime, and an agent writing a
//! second call site has no way to know the first one exists.
//!
//! A typed key moves the check to compile time. It is additive: the `&str` API
//! stays, because same-key-same-type sharing across widgets is a deliberate
//! contract (`{name}.open` on Sheet/Drawer/Popover/Combobox), and typed keys
//! would make that sharing private by construction.

use lumen_core::state::{Runtime, SignalKey};

const COUNT: SignalKey<i64> = SignalKey::new("count");
const LABEL: SignalKey<String> = SignalKey::new("label");

#[test]
fn a_typed_key_addresses_one_signal() {
    let rt = Runtime::new();
    let a = rt.signal_keyed(COUNT, || 0);
    let b = rt.signal_keyed(COUNT, || 0);
    a.set(&rt, 7);
    assert_eq!(b.get(&rt), 7, "the same typed key must be the same signal");
}

#[test]
fn distinct_typed_keys_are_distinct_signals() {
    let rt = Runtime::new();
    let n = rt.signal_keyed(COUNT, || 1i64);
    let s = rt.signal_keyed(LABEL, || String::from("x"));
    n.set(&rt, 42);
    assert_eq!(s.get(&rt), "x", "an unrelated key must be untouched");
}

#[test]
fn typed_and_untyped_access_agree() {
    // The migration property. `SignalKey` hashes as its bare string, so a
    // codebase can convert one call site at a time. If these addressed
    // different slots, migrating a key would silently orphan its state —
    // exactly the failure a typed key is supposed to prevent.
    let rt = Runtime::new();
    let typed = rt.signal_keyed(COUNT, || 0i64);
    typed.set(&rt, 99);

    let untyped = rt.signal("count", || 0i64);
    assert_eq!(
        untyped.get(&rt),
        99,
        "a typed key and the same string must address one signal"
    );
}

#[test]
fn a_typed_key_debug_prints_as_its_bare_key() {
    // Diagnostics, snapshots and the agent's dep names all format keys. A typed
    // key must not change what any of them see, or adopting one would alter
    // the snapshot wire format.
    assert_eq!(format!("{COUNT:?}"), "count");
    assert_eq!(COUNT.as_str(), "count");
}

/// The compile-fail case is the whole point, and cannot be asserted here — it
/// is the absence of a program.
///
/// ```compile_fail
/// # use lumen_core::state::{Runtime, SignalKey};
/// const COUNT: SignalKey<i64> = SignalKey::new("count");
/// let rt = Runtime::new();
/// // Wrong init type for the key's declared type: rejected at compile time,
/// // where the untyped API would have accepted it and panicked at first read.
/// let _ = rt.signal_keyed(COUNT, || String::from("nope"));
/// ```
#[allow(dead_code)]
fn compile_fail_doc() {}
