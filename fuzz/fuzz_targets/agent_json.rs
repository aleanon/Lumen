//! E.3: agent dispatch must never panic — the endpoint parses hostile JSON.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            // One shared app per process: dispatch is what's under test.
            use std::cell::RefCell;
            thread_local! {
                static APP: RefCell<lumen_widgets::Headless> = RefCell::new(
                    lumen_widgets::App::new(|_| lumen_widgets::widgets::text("fuzz"))
                        .run_headless(kurbo::Size::new(100.0, 100.0)),
                );
            }
            APP.with(|a| {
                let _ = lumen_agent::dispatch(&mut a.borrow_mut(), &v);
            });
        }
    }
});
