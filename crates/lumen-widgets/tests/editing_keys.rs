//! The shared editing key map (`text_input::edit_key`): word motion,
//! word deletion, line selection, and the buffer/line split on Home/End.
//!
//! Every text widget in the framework routes its keys through that one
//! function, so a field that behaves differently from `TextInput` is a bug in
//! the widget, not in the key map. `search_and_rich_editors_share_the_map`
//! pins that down.

use kurbo::{Point, Rect, Size};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent};
use lumen_core::semantics::SemanticsNode;
use lumen_widgets::{
    widgets, App, BuildCx, Element, Headless, RichTextEditor, SearchField, TextField, TextInput,
};

fn rect_id(n: &SemanticsNode, id: &str) -> Option<Rect> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| rect_id(c, id))
}

fn click(h: &mut Headless, id: &str) {
    let b = rect_id(&h.semantics_doc().root, id).unwrap_or_else(|| panic!("no #{id}"));
    let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

/// Focus a field and park the caret at the end of the buffer.
///
/// A click both focuses *and* places the caret where it landed — the middle of
/// the box, i.e. the middle of the text. Every test below wants a known start,
/// so they normalise with Ctrl+End rather than depending on the hit point.
fn focus_at_end(h: &mut Headless, id: &str) {
    click(h, id);
    key(h, Key::Named(NamedKey::End), CTRL);
}

fn key(h: &mut Headless, k: Key, mods: Modifiers) {
    h.inject(Event::KeyDown(KeyEvent {
        key: k,
        modifiers: mods,
        repeat: false,
    }));
    h.pump();
}

const CTRL: Modifiers = Modifiers::CTRL;

/// A focused single-line field over `initial`, caret at the end.
fn field(initial: &'static str) -> Headless {
    let mut h = App::new(move |cx: &mut BuildCx| -> Element {
        widgets::column(vec![TextInput::new(cx, "f", initial).id("f").into()])
    })
    .run_headless(Size::new(400.0, 120.0));
    h.pump();
    focus_at_end(&mut h, "f");
    h
}

fn text_of(h: &Headless) -> String {
    TextInput::text_of(h.runtime(), "f")
}

#[test]
fn ctrl_backspace_deletes_the_previous_word() {
    let mut h = field("hello brave new world");
    key(&mut h, Key::Named(NamedKey::Backspace), CTRL);
    assert_eq!(text_of(&h), "hello brave new ");
    key(&mut h, Key::Named(NamedKey::Backspace), CTRL);
    assert_eq!(text_of(&h), "hello brave ");
    // Plain Backspace still deletes one character, not a word.
    key(&mut h, Key::Named(NamedKey::Backspace), Modifiers::empty());
    assert_eq!(text_of(&h), "hello brave");
}

#[test]
fn ctrl_arrows_move_by_word_and_shift_selects_by_word() {
    let mut h = field("alpha beta gamma");
    key(&mut h, Key::Named(NamedKey::ArrowLeft), CTRL);
    key(&mut h, Key::Named(NamedKey::ArrowLeft), CTRL);
    // Caret is now before "beta"; select to the end of the buffer by word.
    key(
        &mut h,
        Key::Named(NamedKey::ArrowRight),
        CTRL | Modifiers::SHIFT,
    );
    key(
        &mut h,
        Key::Named(NamedKey::ArrowRight),
        CTRL | Modifiers::SHIFT,
    );
    // Typing over a selection replaces it.
    h.inject(Event::TextInput(TextInputEvent { text: "X".into() }));
    h.pump();
    assert_eq!(text_of(&h), "alpha X");
}

#[test]
fn ctrl_a_selects_all_without_typing_an_a() {
    let mut h = field("keep me");
    // The chord alone: the shell drops the `text` a command chord resolves to,
    // so no `TextInput` event accompanies it. Typing then replaces everything.
    key(&mut h, Key::Character("a".into()), CTRL);
    h.inject(Event::TextInput(TextInputEvent { text: "Z".into() }));
    h.pump();
    assert_eq!(text_of(&h), "Z", "Ctrl+A must select, not insert an `a`");
}

#[test]
fn ctrl_l_selects_the_line_under_the_caret() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        widgets::column(vec![TextField::new(
            cx,
            "f",
            "first line\nsecond line\nthird",
        )
        .id("f")
        .into()])
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    focus_at_end(&mut h, "f");
    // The caret is at the end of the buffer, i.e. on "third".
    key(&mut h, Key::Character("l".into()), CTRL);
    h.inject(Event::TextInput(TextInputEvent { text: "3rd".into() }));
    h.pump();
    assert_eq!(
        TextInput::text_of(h.runtime(), "f"),
        "first line\nsecond line\n3rd"
    );
}

#[test]
fn home_and_end_are_line_relative_and_ctrl_takes_the_buffer() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        widgets::column(vec![TextField::new(cx, "f", "one\ntwo\nthree")
            .id("f")
            .into()])
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    focus_at_end(&mut h, "f");
    // End of buffer → Home lands at the start of "three", not of "one".
    key(&mut h, Key::Named(NamedKey::Home), Modifiers::empty());
    h.inject(Event::TextInput(TextInputEvent { text: "!".into() }));
    h.pump();
    assert_eq!(TextInput::text_of(h.runtime(), "f"), "one\ntwo\n!three");

    // Ctrl+Home is the buffer start.
    key(&mut h, Key::Named(NamedKey::Home), CTRL);
    h.inject(Event::TextInput(TextInputEvent { text: "@".into() }));
    h.pump();
    assert_eq!(TextInput::text_of(h.runtime(), "f"), "@one\ntwo\n!three");
}

/// `SearchField` and `RichTextEditor` route through the same map, so the word
/// keys work there too — and the search field finally renders the placeholder
/// it is handed instead of dropping it.
#[test]
fn search_and_rich_editors_share_the_map() {
    let mut h = App::new(|cx: &mut BuildCx| -> Element {
        widgets::column(vec![
            SearchField::new(cx, "q", "Search widgets…").id("q").into(),
            RichTextEditor::new(cx, "doc", "alpha beta").into(),
        ])
    })
    .run_headless(Size::new(500.0, 300.0));
    h.pump();

    assert!(
        h.semantics_json().to_string().contains("Search widgets…"),
        "an empty search field shows (and is named by) its placeholder"
    );

    focus_at_end(&mut h, "q-input");
    h.inject(Event::TextInput(TextInputEvent {
        text: "virtual list".into(),
    }));
    h.pump();
    key(&mut h, Key::Named(NamedKey::Backspace), CTRL);
    assert_eq!(TextInput::text_of(h.runtime(), "q"), "virtual ");

    focus_at_end(&mut h, "doc");
    key(&mut h, Key::Named(NamedKey::Backspace), CTRL);
    assert_eq!(TextInput::text_of(h.runtime(), "doc"), "alpha ");
}
