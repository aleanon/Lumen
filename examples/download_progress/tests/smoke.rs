use lumen_core::geometry::Size;

#[test]
fn progress_streams_through_the_sink() {
    let mut a = download_progress::main_app().run_headless(Size::new(420.0, 320.0));
    a.pump();
    assert!(a.semantics_json().to_string().contains("0%"));
    // Start (the inline executor runs the job to completion; every chunk
    // rides the Sink and the last one wins on the next pump).
    let started = a.runtime().signal("started", || false);
    started.set(a.runtime(), true);
    a.pump();
    a.pump();
    assert!(
        a.semantics_json().to_string().contains("100%"),
        "{}",
        a.semantics_json()
    );
}
