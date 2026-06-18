//! T7.3: error boundaries contain a subtree panic (app survives), and the
//! parser/selector engines never panic on fuzzed input.
use kurbo::Size;
use lumen_widgets::boundary::{default_fallback, error_boundary};
use lumen_widgets::{widgets, App, BuildCx, Element, Headless};

fn quiet_panics() {
    std::panic::set_hook(Box::new(|_| {})); // suppress backtraces during catch_unwind
}

#[test]
fn error_boundary_renders_fallback() {
    quiet_panics();
    let el = error_boundary(|| panic!("boom in build"), default_fallback);
    // The boundary returns the fallback, not a crash.
    assert_eq!(el.id.as_ref().unwrap().as_str(), "error-boundary");
    assert!(el.label.contains("boom in build"));
}

fn app(cx: &mut BuildCx) -> Element {
    let _ = cx;
    widgets::column(vec![
        widgets::text("sibling stays alive").id("sibling"),
        // A subtree that panics during build, contained by a boundary.
        error_boundary(
            || panic!("widget exploded"),
            |m| widgets::text(format!("recovered: {m}")).id("fallback"),
        ),
    ])
}

fn label(h: &Headless, id: &str) -> Option<String> {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<String> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.label.clone());
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&h.semantics_doc().root.elided(), id)
}

#[test]
fn app_survives_panicking_subtree() {
    quiet_panics();
    let mut h = App::new(app).run_headless(Size::new(300.0, 120.0));
    h.pump();
    assert_eq!(label(&h, "sibling").as_deref(), Some("sibling stays alive"));
    assert!(
        label(&h, "fallback").unwrap().contains("widget exploded"),
        "subtree recovered"
    );
}

#[test]
fn app_survives_root_build_panic() {
    use lumen_core::events::{Event, PointerButton, PointerEvent, PointerKind};
    use lumen_core::geometry::Point;
    use lumen_core::semantics::{Action, Role};
    use lumen_core::Color;
    use lumen_layout::{Align, Dim, Display, LayoutStyle};
    use std::rc::Rc;
    quiet_panics();

    // The root build panics once a signal flips; a full-window clickable box
    // (no text → it isn't shrunk to text size) flips it.
    let mut h = App::new(|cx: &mut BuildCx| {
        let boom = cx.signal("boom", || false);
        if boom.get(cx.runtime()) {
            panic!("root exploded");
        }
        Element {
            id: Some("boom".into()),
            role: Role::Button,
            background: Some(Color::srgb8(0x30, 0x60, 0x90, 0xff)),
            focusable: true,
            actions: vec![Action::Click],
            on_click: Some(Rc::new(move |rt| boom.set(rt, true))),
            style: LayoutStyle {
                display: Display::Flex,
                width: Dim::pct(1.0),
                height: Dim::pct(1.0),
                align_items: Some(Align::Center),
                justify_content: Some(Align::Center),
                ..LayoutStyle::default()
            },
            children: vec![widgets::text("press").id("ok")],
            ..Element::default()
        }
    })
    .run_headless(Size::new(120.0, 60.0));
    h.pump();
    assert!(label(&h, "ok").is_some(), "good frame built");
    let good = h.screenshot();

    // Click the centre → handler sets the signal → the next build panics.
    let pe = PointerEvent {
        pos: Point::new(60.0, 30.0),
        button: PointerButton::Left,
        pointer: PointerKind::Mouse,
        modifiers: Default::default(),
        click_count: 1,
    };
    h.inject(Event::PointerDown(pe));
    h.inject(Event::PointerUp(pe));
    h.pump(); // routes the click, then the panicking rebuild — contained

    // The window survived: previous frame kept, panic surfaced as a diagnostic.
    assert!(
        h.diagnostics()
            .iter()
            .any(|d| d.code == lumen_core::codes::E0701),
        "contained build panic should surface as E0701"
    );
    assert_eq!(h.screenshot(), good, "last good frame is preserved");
    h.pump(); // still contained, no propagation
}

#[test]
fn parser_and_selector_never_panic_on_fuzz() {
    use lumen_core::semantics::{resolve_one, Role, SemanticsNode};
    // A deterministic pseudo-random byte/char generator.
    let mut state = 0x9e3779b97f4a7c15u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let tree = SemanticsNode::new(1, Role::Window);
    for _ in 0..3000 {
        let len = (next() % 40) as usize;
        let s: String = (0..len)
            .map(|_| char::from(32u8 + (next() % 95) as u8))
            .collect();
        // The .lss parser is total: errors are diagnostics, never panics.
        let _ = lumen_style::parse("fuzz.lss", &s);
        // The selector engine is total too.
        let _ = resolve_one(&tree, &s);
    }
}
