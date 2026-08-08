//! SD6a: a signal key used with two different types must fail with a message
//! that names the key and both types.
//!
//! Signal keys are strings, so this collision is reachable from ordinary code
//! and has no compile-time guard (the typed-key fix is SD6b). Until then the
//! panic text is the entire diagnostic, so it is worth pinning.

use lumen_core::Runtime;

/// Extract the panic message as a string, whatever payload type it used.
fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // keep the test output readable
    let err = std::panic::catch_unwind(f).expect_err("expected a panic");
    std::panic::set_hook(prev);

    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .expect("panic payload was neither String nor &str")
}

#[test]
fn type_mismatch_names_the_key_and_both_types() {
    let msg = panic_message(|| {
        let rt = Runtime::new();
        let _a = rt.signal("shared.key", || 1i64);
        // Same key, different type: the collision SD6a exists to explain.
        let b = rt.signal("shared.key", || String::from("boom"));
        let _ = b.get(&rt);
    });

    assert!(
        msg.contains("shared.key"),
        "message must name the colliding key, got: {msg}"
    );
    assert!(
        msg.contains("i64"),
        "message must name the type already stored, got: {msg}"
    );
    assert!(
        msg.contains("String"),
        "message must name the type just requested, got: {msg}"
    );
}

#[test]
fn type_mismatch_does_not_report_the_box() {
    // `Slot`'s own note warns that calling a StoredValue method through
    // `self.value` resolves the blanket impl on `Box<dyn StoredValue>` —
    // autoref beats deref — and reports the Box's type. That failure mode
    // already shipped once (it broke every downcast in the lean build), and
    // here it would produce a diagnostic naming `Box<dyn StoredValue>` instead
    // of the value type, i.e. exactly the uselessness SD6a removes.
    let msg = panic_message(|| {
        let rt = Runtime::new();
        let _a = rt.signal("k", || 7u8);
        let b = rt.signal("k", || 1.5f32);
        let _ = b.get(&rt);
    });

    assert!(
        !msg.contains("Box<"),
        "diagnostic reported the box rather than the stored type: {msg}"
    );
    assert!(msg.contains("u8"), "expected the stored type u8, got: {msg}");
    assert!(
        msg.contains("f32"),
        "expected the requested type f32, got: {msg}"
    );
}

#[test]
fn same_key_same_type_is_not_an_error() {
    // Deliberate and supported: it is how widget state is shared
    // (`{name}.open` across Sheet/Drawer/Popover/Combobox). SD6a must not
    // turn this into a failure.
    let rt = Runtime::new();
    let a = rt.signal("widget.open", || false);
    let b = rt.signal("widget.open", || false);
    a.set(&rt, true);
    assert!(b.get(&rt), "same key + same type must share one slot");
}
