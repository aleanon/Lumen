//! The reactive data layer: `cx.resource` / `cx.task` feed background results
//! back into state, deterministically, via the `ManualSpawner` (step tasks, then
//! pump to apply). Covers: loading→ready, dep-change refetch keeping the stale
//! value (SWR), blocking streaming into a signal.

use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Size};
use lumen_core::tasks::ManualSpawner;
use lumen_widgets::{App, BuildCx, Element, Headless, TinySkia};

type ManualApp = Headless<TinySkia, ManualSpawner>;

/// Run spawned jobs, then pump so their deferred results apply and the tree
/// rebuilds.
fn settle(a: &mut ManualApp) {
    a.executor().run_pending();
    a.pump();
}

fn click(a: &mut ManualApp, id: &str) {
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
fn resource_loads_then_ready() {
    let build = |cx: &mut BuildCx| {
        let r = cx.resource_blocking::<i32, lumen_widgets::TaskError, _>("n", (), |()| Ok(21 * 2));
        let label = match (r.value, r.loading) {
            (Some(v), _) => format!("value={v}"),
            (None, true) => "loading".to_string(),
            (None, false) => "idle".to_string(),
        };
        Element::text(label)
    };
    let mut a = App::new(build)
        .with_executor(ManualSpawner::new())
        .run_headless(Size::new(100.0, 40.0));

    a.pump();
    assert!(
        a.semantics_json().to_string().contains("loading"),
        "initially loading"
    );

    settle(&mut a);
    assert!(
        a.semantics_json().to_string().contains("value=42"),
        "resolved to 42"
    );
}

#[test]
fn dep_change_refetches_keeping_stale_value() {
    let build = |cx: &mut BuildCx| {
        let id = cx.signal("id", || 1i32);
        let cur = id.get(cx.runtime());
        let r = cx.resource_blocking::<i32, lumen_widgets::TaskError, _>("user", cur, move |id| {
            Ok(id * 10)
        });
        let shown = r
            .value
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".into());

        let mut bump = lumen_widgets::widgets::button("bump", move |rt| id.update(rt, |v| *v += 1));
        bump = bump.id("bump");

        let mut col = lumen_widgets::widgets::column(vec![
            Element::text(format!("val={shown}")),
            Element::text(format!("loading={}", r.loading)),
            bump,
        ]);
        col.style.row_gap = lumen_layout::Dim::px(2.0);
        col
    };
    let mut a = App::new(build)
        .with_executor(ManualSpawner::new())
        .run_headless(Size::new(160.0, 120.0));

    a.pump();
    settle(&mut a);
    assert!(
        a.semantics_json().to_string().contains("val=10"),
        "first load = 10"
    );

    // Bump the dep → refetch. Before the new result applies, the stale value (10)
    // must still show while loading (stale-while-revalidate).
    click(&mut a, "bump"); // id -> 2, then a pump re-runs build
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("val=10") && t.contains("loading=true"),
        "stale value kept while reloading: {t}"
    );

    settle(&mut a);
    assert!(
        a.semantics_json().to_string().contains("val=20"),
        "refetched = 20"
    );
}

#[test]
fn blocking_task_streams_progress_into_a_signal() {
    let build = |cx: &mut BuildCx| {
        let progress = cx.signal("p", || 0i32);
        cx.task_blocking("job", (), move |(), sink| {
            for step in 1..=3 {
                sink.set(progress, step);
            }
        });
        Element::text(format!("p={}", progress.get(cx.runtime())))
    };
    let mut a = App::new(build)
        .with_executor(ManualSpawner::new())
        .run_headless(Size::new(80.0, 40.0));

    a.pump(); // records the task
    a.executor().run_pending(); // streams 1,2,3 onto the channel
    a.pump(); // drains → last value wins
    assert!(
        a.semantics_json().to_string().contains("p=3"),
        "streamed to 3"
    );
}

#[test]
fn boxed_executor_opt_in_compiles_and_runs() {
    // The dynamic-dispatch opt-in for the executor: instantiate with
    // `E = Box<dyn Spawner>` (the blanket `impl Spawner for Box<S>`). Here the
    // boxed executor is the inline one, so a resource settles within two pumps.
    use lumen_core::tasks::{InlineSpawner, Spawner};
    let boxed: Box<dyn Spawner> = Box::new(InlineSpawner);
    let build = |cx: &mut BuildCx| {
        let r = cx.resource_blocking::<i32, lumen_widgets::TaskError, _>("b", (), |()| Ok(5));
        Element::text(r.value.map(|v| v.to_string()).unwrap_or_else(|| "…".into()))
    };
    let mut a = App::new(build)
        .with_executor(boxed)
        .run_headless(Size::new(60.0, 30.0));
    a.pump(); // inline runs the job during dispatch; result queued
    a.pump(); // drains → applied
    assert!(
        a.semantics_json().to_string().contains('5'),
        "boxed inline resolved"
    );
}

/// M.5 (ADR-M2 d): the re-entry contract — a LATE result from a superseded
/// fetch generation must be discarded, not clobber the newer one.
#[test]
fn stale_generation_result_is_discarded() {
    use lumen_core::tasks::ManualSpawner;
    let spawner = ManualSpawner::new();
    let mut h = lumen_widgets::App::new(|cx| {
        let dep = cx.signal("dep", || 1i32);
        let d = dep.get(cx.runtime());
        let r = cx.resource::<String, String, _, _>("data", d, |d| async move {
            Ok(format!("result-for-{d}"))
        });
        lumen_widgets::widgets::text(format!(
            "v={} loading={}",
            r.value.unwrap_or_default(),
            r.loading
        ))
        .id("t")
    })
    .with_executor(spawner.clone())
    .run_headless(kurbo::Size::new(300.0, 100.0));
    h.pump(); // build 1 queues fetch(gen 1) — NOT yet run (manual spawner)

    // The dep changes BEFORE gen 1's job runs: build 2 queues fetch(gen 2).
    let dep = h.runtime().signal("dep", || 0i32);
    dep.set(h.runtime(), 2);
    h.pump();

    // Now run both queued jobs — gen 1 completes LATE, after gen 2 started.
    // Order: gen1 then gen2; gen1's finish must be ignored either way.
    spawner.run_pending();
    h.pump();
    let t = h.semantics_json().to_string();
    assert!(
        t.contains("result-for-2") && !t.contains("result-for-1"),
        "late gen-1 result discarded: {t}"
    );
}
