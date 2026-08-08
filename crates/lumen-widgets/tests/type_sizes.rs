//! CP0.6: pin the size of the structs that are built per node per frame.
//!
//! `Element` is the authoring type — one is constructed for every node on every
//! rebuild — and `NodeMeta` mirrors it in retained storage. Their width is
//! therefore a per-frame cost multiplier, and it drifts silently: a new
//! `Option<Rc<dyn Fn>>` handler slot or an inlined style field costs 8-256
//! bytes on every node and nothing complains.
//!
//! This also replaces a QUARANTINED number. The 2026-08 resource review
//! reported `size_of::<Element>() == 1008`, but that figure came from a
//! field-matched *reconstruction* in a scratch crate, not from the shipped
//! type — nobody had ever measured it in place. These assertions record the
//! real values so later work (EL, CP2.2) can be judged against something
//! measured rather than inferred.

use lumen_widgets::Element;

/// Print-and-assert: the message carries the actual number, so a failure tells
/// you what to update the constant to rather than making you go and measure.
macro_rules! assert_size {
    ($t:ty, $expected:expr) => {{
        let actual = std::mem::size_of::<$t>();
        assert_eq!(
            actual, $expected,
            concat!(
                "size_of::<",
                stringify!($t),
                ">() changed. This type is built per node per frame, so its \
                 width is a direct per-frame cost. If the growth is intended, \
                 update the constant and say why in the commit; if it is not, \
                 this is the regression the assertion exists to catch."
            )
        );
    }};
}

#[test]
fn element_size_is_pinned() {
    // Measured 2026-08-08 on x86_64: 1024 bytes.
    //
    // Note this is NOT the 1008 the resource review reported — that came from a
    // field-matched reconstruction in a scratch crate, and it was 16 bytes off.
    // Small, but it is exactly why the campaign quarantined reconstructed
    // figures: an estimate that looks like a measurement gets cited as one.
    //
    // Reducing this is EL's job; the campaign deprioritized that after finding
    // a 1041-node datagrid's whole Tree+Element footprint (~1.22 MB) is ~200x
    // smaller than the GPU-context tax on the same app. So this is a
    // watch-it-doesn't-grow assertion, not a target.
    //
    // 1024 -> 1040 on 2026-08-08: PROP1 added three fields to the `TextStyle`
    // an `Element` carries inline — `align` (1 byte), `italic` (1 byte) and
    // `features: Option<String>` (24), netting +16 after packing. Accepted
    // rather than shaved:
    //
    //   * it buys `text-align`, `font-style` and `font-features`, three
    //     properties that previously parsed and did nothing;
    //   * at 3 000 nodes it is ~48 KB per frame's element tree, against the
    //     ~270 MB RSS the same app carries — the 200x finding above says this
    //     is not where per-node memory matters;
    //   * `Option<Box<str>>` would recover 8 of the 16 bytes at the cost of
    //     conversion friction on every call site, which is a poor trade for a
    //     figure this far below the noise floor of what dominates.
    //
    // The assertion still earns its place: it made that a decision with a
    // number attached instead of an unnoticed drift.
    assert_size!(Element, 1040);
}

#[test]
fn element_is_not_accidentally_huge() {
    // A cheap upper bound that survives intentional churn: if `Element` ever
    // doubles, something structural went wrong (an inlined collection, a large
    // enum variant) regardless of what the exact constant says.
    let n = std::mem::size_of::<Element>();
    assert!(
        n < 2048,
        "Element grew to {n} bytes — past this point the per-node cost is \
         structural, not incremental"
    );
}
