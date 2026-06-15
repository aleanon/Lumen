//! T1.7 acceptance: edit a `.lss` on disk → the watcher fires, the style changes
//! via `get_styles` (and a reload event is produced) within 500 ms; a broken
//! edit keeps the old style and yields an E0101 reload event.

use lumen_cli::dev::{tier1_reload, watch_file};
use lumen_widgets::{widgets, App, ReloadResult};
use std::time::Duration;

fn bg(h: &lumen_widgets::Headless) -> Option<String> {
    h.get_styles("#b")
        .get("background")?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

fn tmpdir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "lumen-hot-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn lss_on_disk_hot_reloads_within_500ms() {
    let dir = tmpdir();
    let path = dir.join("app.lss");
    std::fs::write(&path, "button { background: #ff0000ff; }").unwrap();

    let mut h = App::new(|_| widgets::button("Hi", |_| {}).id("b"))
        .stylesheet("button { background: #ff0000ff; }")
        .run_headless(lumen_core::geometry::Size::new(200.0, 80.0));
    h.pump();
    assert_eq!(bg(&h).as_deref(), Some("#ff0000ff"));

    let (_watcher, rx) = watch_file(&path).expect("watch");

    // --- valid edit -> style changes + reload "ok" within 500 ms ---
    std::fs::write(&path, "button { background: #0000ffff; }").unwrap();
    rx.recv_timeout(Duration::from_millis(500))
        .expect("file change within 500ms");
    let src = std::fs::read_to_string(&path).unwrap();
    let event = tier1_reload(&src);
    assert_eq!(event.status, "ok");
    assert!(matches!(h.set_stylesheet(&src), ReloadResult::Ok));
    assert_eq!(bg(&h).as_deref(), Some("#0000ffff"), "style changed live");

    // --- broken edit -> E0101 event, old style stays ---
    std::fs::write(&path, "button { background: }").unwrap();
    // drain any signal(s) for this write
    let _ = rx.recv_timeout(Duration::from_millis(500));
    let src = std::fs::read_to_string(&path).unwrap();
    let event = tier1_reload(&src);
    assert_eq!(event.status, "error");
    assert!(event.diagnostics.iter().any(|d| d.code == "E0101"));
    assert!(matches!(h.set_stylesheet(&src), ReloadResult::Failed(_)));
    assert_eq!(
        bg(&h).as_deref(),
        Some("#0000ffff"),
        "broken edit keeps the old style"
    );

    std::fs::remove_dir_all(&dir).ok();
}
