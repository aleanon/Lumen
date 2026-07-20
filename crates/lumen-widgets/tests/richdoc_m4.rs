//! M.4: RichDoc model (lists/links/images, round-trip), source editing with
//! the full caret/selection machinery, and the find/replace UI.

use kurbo::Size;
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent};
use lumen_core::geometry::Point;
use lumen_widgets::richdoc::{Block, RichDoc};
use lumen_widgets::{widgets, widgets_m4, App, Headless};

const DOC: &str = "# Title\n\
Intro with **bold** and *italic* and a [link](https://lumen.dev).\n\
- first bullet\n\
- second bullet\n\
1. numbered one\n\
2. numbered two\n\
![diagram](assets/d.png)\n";

#[test]
fn parses_blocks_spans_and_round_trips() {
    let doc = RichDoc::parse(DOC);
    assert_eq!(doc.blocks.len(), 7);
    assert!(matches!(&doc.blocks[0], Block::Heading(1, s) if s[0].text == "Title"));
    match &doc.blocks[1] {
        Block::Paragraph(s) => {
            assert!(s.iter().any(|x| x.bold && x.text == "bold"));
            assert!(s.iter().any(|x| x.italic && x.text == "italic"));
            assert!(s
                .iter()
                .any(|x| x.link.as_deref() == Some("https://lumen.dev") && x.text == "link"));
        }
        b => panic!("expected paragraph, got {b:?}"),
    }
    assert!(matches!(&doc.blocks[2], Block::Bullet(_)));
    assert!(matches!(&doc.blocks[4], Block::Numbered(1, _)));
    assert!(
        matches!(&doc.blocks[6], Block::Image { alt, src } if alt == "diagram" && src == "assets/d.png")
    );
    // Round trip.
    assert_eq!(RichDoc::parse(&doc.to_source()), doc);
}

#[test]
fn renders_links_lists_and_images_semantically() {
    let mut h = App::new(|cx| {
        let clicked = cx.signal("url", String::new);
        let doc = RichDoc::parse(DOC);
        widgets::column(vec![
            doc.render(move |rt, url| clicked.set(rt, url.to_string())),
            widgets::text(format!("clicked: {}", clicked.get(cx.runtime()))).id("out"),
        ])
    })
    .run_headless(Size::new(420.0, 400.0));
    h.pump();
    let t = h.semantics_json().to_string();
    assert!(t.contains("\"link\""), "link role present");
    assert!(t.contains("•"), "bullet marker rendered");
    assert!(t.contains("2."), "numbered marker rendered");
    assert!(t.contains("diagram"), "image alt is the accessible name");

    // Click the link → the handler receives the url.
    let root = h.semantics_doc().root.elided();
    fn find_link(n: &lumen_core::semantics::SemanticsNode) -> Option<kurbo::Rect> {
        if n.role == lumen_core::semantics::Role::Link {
            return Some(n.bounds);
        }
        n.children.iter().find_map(find_link)
    }
    let b = find_link(&root).expect("link node");
    let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    assert!(
        h.semantics_json()
            .to_string()
            .contains("clicked: https://lumen.dev"),
        "link handler fired"
    );
}

fn key(h: &mut Headless, k: Key, mods: Modifiers) {
    h.inject(Event::KeyDown(KeyEvent {
        key: k,
        modifiers: mods,
        repeat: false,
    }));
    h.pump();
}

#[test]
fn editor_caret_selection_editing_on_the_source() {
    let mut h =
        App::new(|cx| widgets::column(vec![widgets_m4::rich_text_editor(cx, "doc", "# Hi")]))
            .run_headless(Size::new(500.0, 300.0));
    h.pump();

    // Focus the source pane by clicking it.
    let root = h.semantics_doc().root.elided();
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let b = find(&root, "doc").expect("source pane");
    let p = Point::new((b.x0 + b.x1) / 2.0, b.y0 + 10.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();

    // Type at the end, then use caret ops: End, select-left twice, delete.
    h.inject(Event::TextInput(lumen_core::events::TextInputEvent {
        text: "!!".into(),
    }));
    h.pump();
    let src = h.runtime().signal("doc.text", String::new).get(h.runtime());
    assert!(src.ends_with("!!"), "insert at caret: {src}");

    key(&mut h, Key::Named(NamedKey::End), Modifiers::empty());
    key(&mut h, Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
    key(&mut h, Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
    key(&mut h, Key::Named(NamedKey::Backspace), Modifiers::empty());
    let src = h.runtime().signal("doc.text", String::new).get(h.runtime());
    assert_eq!(src, "# Hi", "selection deleted: {src}");

    // The live preview parsed the heading.
    let t = h.semantics_json().to_string();
    assert!(t.contains("doc-preview"), "preview present");
}

#[test]
fn find_replace_bar_counts_and_rewrites() {
    let mut h = App::new(|cx| {
        widgets::column(vec![
            widgets_m4::rich_text_editor(cx, "doc", "good day, good night"),
            widgets_m4::find_replace_bar(cx, "fr", "doc"),
        ])
    })
    .run_headless(Size::new(600.0, 400.0));
    h.pump();

    // Set the find/replace inputs through their signals (what typing does).
    let find = h.runtime().signal("fr.find", String::new);
    find.set(h.runtime(), "good".into());
    h.pump();
    assert!(
        h.semantics_json().to_string().contains("2 match(es)"),
        "live count"
    );
    let rep = h.runtime().signal("fr.replace", String::new);
    rep.set(h.runtime(), "great".into());
    h.pump();

    // Click "Replace all".
    let root = h.semantics_doc().root.elided();
    fn find_id(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| find_id(c, id))
    }
    let b = find_id(&root, "fr-apply").expect("apply button");
    let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();

    let src = h.runtime().signal("doc.text", String::new).get(h.runtime());
    assert_eq!(src, "great day, great night");
    assert!(h.semantics_json().to_string().contains("0 match(es)"));
}
