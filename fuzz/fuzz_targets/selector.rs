//! E.3: the selector grammar must never panic — agents send arbitrary
//! selector strings over the wire. Resolved against a real semantic tree.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::cell::RefCell;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        thread_local! {
            static ROOT: RefCell<lumen_core::semantics::SemanticsNode> = RefCell::new({
                let mut h = lumen_widgets::App::new(|_| {
                    lumen_widgets::widgets::column(vec![
                        lumen_widgets::widgets::text("fuzz").id("t"),
                        lumen_widgets::widgets::button("b", |_| {}).id("b"),
                    ])
                })
                .run_headless(kurbo::Size::new(100.0, 100.0));
                h.pump();
                h.semantics_doc().root.elided()
            });
        }
        ROOT.with(|r| {
            let _ = lumen_core::semantics::resolve_one(&r.borrow(), s);
        });
    }
});
