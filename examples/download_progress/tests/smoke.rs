use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_core::tasks::{ManualSpawner, Spawner};
use lumen_widgets::{Headless, TinySkia};

fn click<E: Spawner>(a: &mut Headless<TinySkia, E>, id: &str) {
    fn find(
        n: &lumen_core::semantics::SemanticsNode,
        id: &str,
    ) -> Option<lumen_core::geometry::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let b = find(&a.semantics_doc().root, id).unwrap_or_else(|| panic!("no node {id}"));
    let pe = PointerEvent {
        pos: Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
}

#[test]
fn progress_streams_through_the_sink() {
    let mut a = download_progress::main_app().run_headless(Size::new(420.0, 320.0));
    a.pump();
    assert!(a.semantics_json().to_string().contains("idle"));
    // Start (the inline executor runs the job to completion; every chunk
    // rides the Sink and the last one wins on the next pump).
    click(&mut a, "start");
    a.pump();
    assert!(
        a.semantics_json().to_string().contains("done"),
        "{}",
        a.semantics_json()
    );
}

/// TC1: Cancel aborts the transfer. Driven with the `ManualSpawner` so the
/// window between "task queued" and "task ran" is ours to control — with the
/// inline executor the download completes inside `pump`, before any click.
#[test]
fn cancel_stops_the_transfer() {
    let spawner = ManualSpawner::new();
    let mut a = download_progress::main_app()
        .with_executor(spawner.clone())
        .run_headless(Size::new(420.0, 320.0));
    a.pump();

    click(&mut a, "start");
    assert_eq!(spawner.pending(), 1, "one transfer queued");

    click(&mut a, "cancel");
    assert_eq!(spawner.pending(), 0, "the queued transfer was dropped");

    spawner.run_pending();
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("cancelled at 0%"), "{t}");
}

/// After a cancel, Start must work again: the run counter is the task's deps, so
/// a new run is a new generation rather than the same (already cancelled) task.
#[test]
fn start_after_cancel_begins_a_new_transfer() {
    let spawner = ManualSpawner::new();
    let mut a = download_progress::main_app()
        .with_executor(spawner.clone())
        .run_headless(Size::new(420.0, 320.0));
    a.pump();
    click(&mut a, "start");
    click(&mut a, "cancel");
    assert_eq!(spawner.pending(), 0);

    click(&mut a, "start");
    assert_eq!(spawner.pending(), 1, "a fresh generation was spawned");
    spawner.run_pending();
    a.pump();
    assert!(
        a.semantics_json().to_string().contains("done"),
        "{}",
        a.semantics_json()
    );
}
