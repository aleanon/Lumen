//! Full text-editor behavior for TextInput/TextField, driven through the real
//! input queue: caret placement, typing at the cursor, selection, clipboard,
//! undo/redo, multi-line newline + vertical nav, and that focusing a field
//! actually paints something (caret/focus ring).

use kurbo::{Point, Size};
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent};
use lumen_widgets::{App, BuildCx, Element, Headless, TextField, TextInput};

fn run(build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(240.0, 80.0))
}

fn value(h: &Headless) -> String {
    h.semantics_doc().root.elided().value.unwrap_or_default()
}

fn bounds(h: &Headless) -> kurbo::Rect {
    h.semantics_doc().root.elided().bounds
}

fn click_at(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn typ(h: &mut Headless, text: &str) {
    h.inject(Event::TextInput(TextInputEvent {
        text: text.to_string(),
    }));
    h.pump();
}

fn key(h: &mut Headless, named: NamedKey, mods: Modifiers) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers: mods,
        repeat: false,
    }));
    h.pump();
}

fn ctrl(h: &mut Headless, ch: &str) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Character(ch.into()),
        modifiers: Modifiers::CTRL,
        repeat: false,
    }));
    h.pump();
}

fn input(initial: &'static str) -> Headless {
    run(move |cx| TextInput::new(cx, "in", initial).id("in").into())
}

/// Focus the field by clicking, placing the caret at the end (past the text) so
/// it matches `TextEditor`'s initial end-cursor. Keys/text only route to the
/// focused node.
fn focus_end(h: &mut Headless) {
    let b = bounds(h);
    click_at(h, Point::new(b.x1 - 4.0, (b.y0 + b.y1) / 2.0));
}

#[test]
fn click_places_caret_and_types_there() {
    let mut h = input("hello");
    assert_eq!(value(&h), "hello");
    // Click just inside the left padding → caret before the first glyph.
    let b = bounds(&h);
    click_at(&mut h, Point::new(b.x0 + 9.0, (b.y0 + b.y1) / 2.0));
    typ(&mut h, "X");
    assert_eq!(
        value(&h),
        "Xhello",
        "typed at the click-placed caret (start)"
    );
}

#[test]
fn arrows_and_deletion_edit_mid_string() {
    let mut h = input("abc");
    focus_end(&mut h); // focus; caret at the end
    key(&mut h, NamedKey::ArrowLeft, Modifiers::empty()); // between b|c
    key(&mut h, NamedKey::Backspace, Modifiers::empty()); // delete 'b'
    assert_eq!(value(&h), "ac");
    typ(&mut h, "B"); // reinsert at the cursor
    assert_eq!(value(&h), "aBc");
    key(&mut h, NamedKey::Home, Modifiers::empty());
    key(&mut h, NamedKey::Delete, Modifiers::empty()); // delete 'a'
    assert_eq!(value(&h), "Bc");
}

#[test]
fn shift_selection_then_typing_replaces() {
    let mut h = input("hello");
    focus_end(&mut h);
    // From the end, Shift+Left twice selects "lo".
    key(&mut h, NamedKey::ArrowLeft, Modifiers::SHIFT);
    key(&mut h, NamedKey::ArrowLeft, Modifiers::SHIFT);
    typ(&mut h, "X"); // replaces the selection
    assert_eq!(value(&h), "helX");
}

#[test]
fn select_all_copy_paste() {
    let mut h = input("hi");
    let b = bounds(&h);
    click_at(&mut h, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
    ctrl(&mut h, "a"); // select all
    ctrl(&mut h, "c"); // copy "hi"
    key(&mut h, NamedKey::End, Modifiers::empty()); // collapse to end
    ctrl(&mut h, "v"); // paste
    assert_eq!(value(&h), "hihi");
}

#[test]
fn undo_redo() {
    let mut h = input("");
    focus_end(&mut h);
    typ(&mut h, "a");
    typ(&mut h, "b");
    assert_eq!(value(&h), "ab");
    ctrl(&mut h, "z");
    assert_eq!(value(&h), "a");
    ctrl(&mut h, "y");
    assert_eq!(value(&h), "ab");
}

#[test]
fn focusing_paints_caret_and_ring() {
    let mut h = input("hi");
    let unfocused = h.screenshot();
    let b = bounds(&h);
    click_at(&mut h, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
    let focused = h.screenshot();
    assert!(
        unfocused.diff_count(&focused) > 0,
        "focusing a field must paint a caret / focus ring"
    );
}

#[test]
fn multiline_enter_inserts_newline_and_vertical_nav() {
    let mut h = run(|cx| TextField::new(cx, "tf", "xxxx").id("tf").into());
    focus_end(&mut h);
    // Caret at end of "xxxx"; Enter adds a line, then type on line 2.
    key(&mut h, NamedKey::End, Modifiers::empty());
    key(&mut h, NamedKey::Enter, Modifiers::empty());
    typ(&mut h, "yyyy");
    assert_eq!(value(&h), "xxxx\nyyyy");
    // From the end of line 2, ArrowUp lands on line 1; typing inserts there.
    key(&mut h, NamedKey::ArrowUp, Modifiers::empty());
    typ(&mut h, "Z");
    let v = value(&h);
    let (l1, l2) = v.split_once('\n').unwrap();
    assert!(
        l1.contains('Z'),
        "ArrowUp moved the caret to line 1 (got {v:?})"
    );
    assert_eq!(l2, "yyyy", "line 2 untouched (got {v:?})");
}
