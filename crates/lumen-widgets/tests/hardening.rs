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
