//! The showcase's contract: every catalogued widget renders cleanly, the
//! dropdown lists all of them, and picking one swaps the stage.
use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
use lumen_core::geometry::{Point, Rect, Size};
use lumen_core::semantics::SemanticsNode;
use lumen_core::state::Signal;
use lumen_widgets::Headless;

const W: f64 = 980.0;
const H: f64 = 760.0;

fn app() -> Headless {
    let mut a = widget_showcase::main_app().run_headless(Size::new(W, H));
    a.pump();
    a
}

fn rect_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}

fn has_id(a: &Headless, id: &str) -> bool {
    rect_id(&a.semantics_doc().root, id).is_some()
}

/// Click and settle.
///
/// The second pump is not superstition. A pointer press changes visual state
/// (`pressed`), which sends that pump down the restyle-only path — and that
/// path does not settle text bindings, so a bound readout whose signal moved in
/// the same pump is still showing the old string when it returns. The next
/// pump has no visual delta, falls through to the binding check, and patches.
/// A live window pumps again on its own; a test has to ask.
fn press(a: &mut Headless, p: Point) {
    let pe = PointerEvent {
        pos: p,
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    a.inject(Event::PointerDown(pe));
    a.inject(Event::PointerUp(pe));
    a.pump();
    a.pump();
}

fn click(a: &mut Headless, id: &str) {
    let b = rect_id(&a.semantics_doc().root, id).unwrap_or_else(|| panic!("no #{id}"));
    press(a, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
}

/// Select a widget by name without going through the dropdown — the store is
/// the source of truth, so tests for a single demo need not scroll the panel.
fn select(a: &mut Headless, name: &str) {
    let sig: Signal<String> = a.runtime().signal("widget", String::new);
    sig.set(a.runtime(), name.to_string());
    a.pump();
}

/// Findings that belong to the framework's own widgets, not to this gallery.
///
/// Each is something a caller cannot fix from the outside, and each is a true
/// positive worth keeping visible — hence the list rather than a blanket
/// `allow`. Every other code, on every other widget, must come back clean.
fn known_framework_finding(widget: &str, code: &str) -> bool {
    match code {
        // `Badge` pins its pill at `top: -9, right: -14` of its own wrapper.
        // Overhanging the target's corner *is* the widget, and insets resolve
        // against the border box, so no caller-side padding absorbs it.
        "W0103" if widget == "Badge" => true,
        // `Grid`'s viewport sets `clip: true` but publishes no `ScrollInfo`,
        // and the audit's scroll-container exemption keys off an ancestor
        // `scroll`, not `clip`. So the grid's one row of overscan — clipped,
        // never painted — is reported both as overflowing the viewport (W0103)
        // and as white-on-page-background text (W0303), while `VirtualList`,
        // `DataGrid` and `Scrollable`, which all publish `scroll`, are exempt.
        "W0103" | "W0303" if widget == "Grid" => true,
        _ => false,
    }
}

#[test]
fn every_widget_renders_and_is_lint_clean() {
    let mut a = app();
    let mut bad: Vec<String> = Vec::new();
    for entry in widget_showcase::catalog::all() {
        select(&mut a, entry.name);
        assert!(
            rect_id(&a.semantics_doc().root, "stage").is_some(),
            "{}: the stage vanished",
            entry.name
        );
        for d in a.lint() {
            if !known_framework_finding(entry.name, d.code) {
                bad.push(format!("{}: [{}] {}", entry.name, d.code, d.message));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "visual-invariant findings:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_picker_lists_every_widget() {
    let mut a = app();
    click(&mut a, "widget-picker");
    assert!(has_id(&a, "picker-panel"), "the panel should be open");
    for entry in widget_showcase::catalog::all() {
        assert!(
            has_id(&a, &format!("opt-{}", entry.slug)),
            "{} is missing from the dropdown",
            entry.name
        );
    }
}

/// Picking a row swaps the stage — *and* proves the row wins the click.
///
/// The open panel hangs down over the stage, so this row's centre lies inside
/// the stage's box. `Element::overlay` only moves paint order; hit-testing
/// still follows document order, so the header has to be the later sibling or
/// this click would land on the stage instead.
#[test]
fn picking_a_row_swaps_the_stage_and_beats_it_to_the_click() {
    let mut a = app();
    assert!(has_id(&a, "btn-primary"), "the default demo is Button");

    click(&mut a, "widget-picker");
    // A row far enough down the panel to sit over the stage, but still inside
    // the scrolling viewport — a clipped row would take no click at all.
    let row = rect_id(&a.semantics_doc().root, "opt-avatar").expect("no #opt-avatar");
    let panel = rect_id(&a.semantics_doc().root, "picker-panel").expect("no #picker-panel");
    assert!(
        row.y0 > 116.0,
        "the row must overlap the stage for this to test anything (y0 = {})",
        row.y0
    );
    assert!(
        row.y1 < panel.y1,
        "the row must be inside the panel's viewport (row {row:?}, panel {panel:?})"
    );

    click(&mut a, "opt-avatar");
    assert!(!has_id(&a, "picker-panel"), "picking closes the panel");
    assert!(!has_id(&a, "btn-primary"), "the Button demo is gone");
    assert!(
        a.semantics_json().to_string().contains("Ada Lovelace"),
        "the Avatar demo is on the stage"
    );
}

#[test]
fn the_status_readout_tracks_the_store() {
    let mut a = app();
    assert!(a.semantics_json().to_string().contains("presses: 0"));
    click(&mut a, "btn-primary");
    click(&mut a, "btn-default");
    assert!(
        a.semantics_json().to_string().contains("presses: 2"),
        "the header readout follows the demo's state"
    );
}

#[test]
fn a_disabled_widget_stays_inert() {
    let mut a = app();
    click(&mut a, "btn-disabled");
    assert!(
        a.semantics_json().to_string().contains("presses: 0"),
        "a disabled button must not fire its handler"
    );
}

#[test]
fn stateful_demos_seed_their_signals() {
    let mut a = app();
    for (widget, want) in [
        ("Slider", "volume 65%"),
        ("RangeSlider", "240 – 780 kr"),
        ("Stepper", "quantity = 3"),
        ("Radio", "plan = standard"),
        ("PickList", "city = Trondheim"),
        ("DatePicker", "2026-08-24"),
        ("TimePicker", "14:45"),
        ("Pagination", "page 2 of 5"),
        ("ColorPicker", "brand = #18a05c"),
    ] {
        select(&mut a, widget);
        assert!(
            a.semantics_json().to_string().contains(want),
            "{widget} should start seeded with {want:?}"
        );
    }
}

#[test]
fn slugs_and_names_are_unique() {
    let mut names: Vec<&str> = widget_showcase::catalog::all().map(|e| e.name).collect();
    let mut slugs: Vec<&str> = widget_showcase::catalog::all().map(|e| e.slug).collect();
    let (n, s) = (names.len(), slugs.len());
    names.sort_unstable();
    names.dedup();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(names.len(), n, "duplicate widget name");
    assert_eq!(slugs.len(), s, "duplicate widget slug");
    for slug in slugs {
        assert!(
            slug.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "slug {slug:?} must be [a-z0-9-] — a dot would parse as id+class and be unselectable"
        );
    }
}
