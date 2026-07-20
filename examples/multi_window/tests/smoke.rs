use lumen_core::geometry::Size;

#[test]
fn preview_window_tracks_the_shared_note() {
    let mut main = multi_window::main_app().run_headless(Size::new(420.0, 320.0));
    main.pump();
    let mut preview = main.open_window("preview").expect("declared window");
    assert!(preview.semantics_json().to_string().contains("» hello"));

    // Edit through the shared store (what typing in main does).
    let note = main.runtime().signal("note", String::new);
    note.set(main.runtime(), "from main".into());
    main.pump();
    preview.pump();
    assert!(
        preview.semantics_json().to_string().contains("» from main"),
        "preview re-rendered from the shared signal"
    );
}
