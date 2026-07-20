//! E.3: the `.lss` parser must never panic — malformed stylesheets are data
//! (diagnostics), not crashes (hot-reload feeds arbitrary editor states here).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = lumen_style::parser::parse("fuzz.lss", s);
    }
});
