//! E.3: the crash-report hook converts an escaping panic into a structured
//! E0702 diagnostic delivered to the sink before the process would die.
use lumen_core::diagnostics::{codes, install_crash_hook};
use std::sync::mpsc;

#[test]
fn panic_reaches_the_sink_as_e0702() {
    let (tx, rx) = mpsc::channel();
    install_crash_hook(move |d| {
        let _ = tx.send(d);
    });
    // Panic on a scratch thread — the hook is process-wide.
    let _ = std::thread::spawn(|| panic!("boom in a worker")).join();
    let d = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("sink received the report");
    assert_eq!(d.code, codes::E0702);
    assert!(d.message.contains("boom in a worker"));
    assert!(
        d.message.contains("crash_hook.rs"),
        "location captured: {}",
        d.message
    );
    // Restore quiet default for the rest of the suite.
    let _ = std::panic::take_hook();
}
