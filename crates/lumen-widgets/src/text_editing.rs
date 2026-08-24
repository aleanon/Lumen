//! Rich text: styled runs, an editable rich-text surface, and its find/replace
//! bar.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Element};
use lumen_core::semantics::{Action, Role};
use lumen_core::Color;
use lumen_layout::{Dim, Display, Edges, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// One styled run of [`rich_text`].
pub struct Run<'a> {
    /// Text content.
    pub text: &'a str,
    /// Run colour.
    pub color: Color,
    /// Font size (px).
    pub size: f32,
}

/// [`RichText`] — a row of differently-styled text runs (typed form of
/// [`rich_text`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::widgets::Run;
/// use lumen_widgets::{centered, RichText, BuildCx, Element};
/// use lumen_core::Color;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let runs = [
///         Run { text: "Bold ", color: Color::BLACK, size: 16.0 },
///         Run { text: "and blue", color: Color::srgb8(0x1a, 0x73, 0xe8, 0xff), size: 16.0 },
///     ];
///     centered(cx, RichText::new(&runs).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 220.0, 56.0, "rich_text");
/// ```
///
/// Renders:
///
/// ![Rich Text example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/rich_text.png)
///
/// The picture above is `src/doc_shots/rich_text.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct RichText {
    /// The runs, owned. `Run<'a>` borrows, so the widget cannot hold one past
    /// the call — and it already copied each `&str` into the node anyway.
    runs: Vec<(String, Color, f32)>,
    common: Common,
}

impl RichText {
    /// A row of differently-styled runs.
    pub fn new(runs: &[Run]) -> RichText {
        RichText {
            runs: runs
                .iter()
                .map(|r| (r.text.to_string(), r.color, r.size))
                .collect(),
            common: Common::default(),
        }
    }
}

impl Widget for RichText {
    fn build(self) -> Element {
        let RichText { runs, common } = self;
        let label = runs.iter().map(|(t, _, _)| t.as_str()).collect::<String>();
        let children = runs
            .into_iter()
            .map(|(text, color, size)| Element {
                role: Role::Text,
                label: text.clone(),
                content: crate::NodeContent::Text(
                    text,
                    TextStyle {
                        font_size: size,
                        weight: 400.0,
                        color,
                        line_height: None,
                        letter_spacing: 0.0,
                        family: None,
                        features: None,
                        variations: None,
                        italic: false,
                        align: Default::default(),
                    },
                ),
                ..Element::default()
            })
            .collect();
        let mut el = Element {
            role: Role::Group,
            label,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Dim::px(2.0),
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(RichText);

/// A paragraph of differently-styled runs laid out in a row.
/// *(Thin shim over [`RichText`] — the typed form is preferred.)*
pub fn rich_text(runs: &[Run]) -> Element {
    RichText::new(runs).into()
}

/// [`RichTextEditor`] — a markdown-lite source editor with a live parsed
/// preview; state under `name` (typed form of [`rich_text_editor`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, RichTextEditor, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, RichTextEditor::new(cx, "doc", "# Title\nSome **bold** body").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 320.0, 140.0, "rich_text_editor");
/// ```
///
/// Renders:
///
/// ![Rich Text Editor example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/rich_text_editor.png)
///
/// The picture above is `src/doc_shots/rich_text_editor.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct RichTextEditor {
    /// Built where the `BuildCx` is. Every part of this widget needs it: the
    /// editor signal, the mirror, and the `RichDoc` parse of the *current*
    /// source that feeds the live preview. Nothing is left to defer.
    el: Element,
    common: Common,
}

impl RichTextEditor {
    /// A markdown-lite source pane with a live preview beneath it.
    pub fn new(cx: &BuildCx, name: &str, initial: &str) -> RichTextEditor {
        use lumen_text::TextEditor;
        let editor = cx.signal(name, || TextEditor::new(initial));
        let mirror = cx.signal(format!("{name}.text"), || initial.to_string());
        let ed = editor.get(cx.runtime());
        let src = ed.text().to_string();
        let shown = if src.is_empty() {
            " ".to_string()
        } else {
            src.clone()
        };

        // The source pane: real caret + selection on the markdown-lite source.
        let source_pane = Element {
            role: Role::TextInput,
            focusable: true,
            // An I-beam, like every other editable surface.
            cursor: Some(lumen_core::CursorShape::Text),
            label: src.clone(),
            value: Some(src.clone()),
            actions: vec![Action::Focus, Action::SetValue],
            background: Some(Color::srgb8(0xf7, 0xf8, 0xfa, 0xff)),
            corner_radius: 6.0,
            // Matches `TextInput`: a filled box with no edge reads as a label.
            border: Some(lumen_render::Border {
                width: 1.0,
                color: Color::srgb8(0xc9, 0xd0, 0xdb, 0xff),
            }),
            style: LayoutStyle {
                padding: Edges::all(Dim::px(6.0)),
                min_width: Dim::px(220.0),
                min_height: Dim::px(56.0),
                width: Dim::px(300.0),
                ..LayoutStyle::default()
            },
            content: crate::NodeContent::Text(shown, lumen_text::TextStyle::default()),
            caret_byte: Some(ed.cursor()),
            selection: ed.has_selection().then(|| ed.selection()),
            on_text: Some(Rc::new(move |rt, t| {
                editor.update(rt, |e| e.insert(t));
                mirror.set(rt, editor.get(rt).text().to_string());
            })),
            on_caret_set: Some(Rc::new(move |rt, byte, extend| {
                editor.update(rt, |e| e.place(byte, extend));
            })),
            // W2: `SetValue` is advertised, so implement it — replace the
            // whole source through the normal edit path so undo stays
            // coherent. Without this the agent could type into the editor
            // but never set it.
            on_set_value: Some(Rc::new(move |rt, v: &str| {
                editor.update(rt, |e| {
                    e.select_all();
                    e.insert(v);
                });
                mirror.set(rt, editor.get(rt).text().to_string());
            })),
            on_key: Some(Rc::new(move |rt, ke| {
                crate::text_input::edit_key(rt, ke, editor, mirror, true);
            })),
            ..Element::default()
        }
        .id(name);

        // The live preview: the parsed RichDoc (lists, links, images).
        let doc = crate::richdoc::RichDoc::parse(&src);
        let preview = doc.render(|_, _| {}).id(format!("{name}-preview"));

        let mut col = crate::widgets::column(vec![source_pane, preview]);
        col.style.row_gap = Dim::px(8.0);
        RichTextEditor {
            el: col,
            common: Common::default(),
        }
    }
}

impl Widget for RichTextEditor {
    fn build(self) -> Element {
        let RichTextEditor { mut el, common } = self;
        common.apply(&mut el);
        el
    }
}

impl_widget!(RichTextEditor);

/// M.4: the rich-text editor — the `RichDoc` model edited at the SOURCE
/// level with the full [`lumen_text::TextEditor`] caret/selection/clipboard/
/// undo machinery (same engine as `TextField`), plus a live parsed preview.
/// State: `{name}` holds the `TextEditor`; `{name}.text` mirrors the source
/// for plain reads. The semantic value is the source; the preview subtree
/// carries the rendered document (links/lists/images per [`crate::richdoc`]).
/// *(Thin shim over [`RichTextEditor`] — the typed form is preferred.)*
pub fn rich_text_editor(cx: &BuildCx, name: &str, initial: &str) -> Element {
    RichTextEditor::new(cx, name, initial).into()
}

/// [`FindReplaceBar`] — find/replace over a [`RichTextEditor`]'s source;
/// inputs under `name` (typed form of [`find_replace_bar`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, widgets, FindReplaceBar, RichTextEditor, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let col = widgets::column(vec![
///         RichTextEditor::new(cx, "doc", "hello world").into(),
///         FindReplaceBar::new(cx, "fr", "doc").into(),
///     ]);
///     centered(cx, col)
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 400.0, 170.0, "find_replace_bar");
/// ```
///
/// Renders:
///
/// ![Find Replace Bar example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/find_replace_bar.png)
///
/// The picture above is `src/doc_shots/find_replace_bar.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct FindReplaceBar {
    /// Built where the `BuildCx` is: the match count is a search over the
    /// editor's *current* text, and the two fields are `TextInput`s that own
    /// signals. As with [`RichTextEditor`], there is nothing left to defer.
    el: Element,
    common: Common,
}

impl FindReplaceBar {
    /// A find/replace strip bound to the editor named `editor_name`.
    pub fn new(cx: &BuildCx, name: &str, editor_name: &str) -> FindReplaceBar {
        use lumen_text::TextEditor;
        let editor = cx.signal(editor_name, || TextEditor::new(""));
        let mirror = cx.signal(format!("{editor_name}.text"), String::new);
        let find_key = format!("{name}.find");
        let replace_key = format!("{name}.replace");

        let needle = crate::TextInput::text_of(cx.runtime(), &find_key);
        let count = crate::richdoc::RichDoc::find(&mirror.get(cx.runtime()), &needle).len();

        let apply = {
            let find_key = find_key.clone();
            let replace_key = replace_key.clone();
            move |rt: &lumen_core::state::Runtime| {
                let needle = crate::TextInput::text_of(rt, &find_key);
                let with = crate::TextInput::text_of(rt, &replace_key);
                if needle.is_empty() {
                    return;
                }
                editor.update(rt, |e| {
                    let (next, n) = crate::richdoc::RichDoc::replace_all(e.text(), &needle, &with);
                    if n > 0 {
                        e.select_all();
                        e.insert(&next);
                    }
                });
                mirror.set(rt, editor.get(rt).text().to_string());
            }
        };

        let label = match count {
            0 if needle.is_empty() => "no search term".to_string(),
            0 => "no matches".to_string(),
            1 => "1 match".to_string(),
            n => format!("{n} matches"),
        };
        let mut count_el = crate::widgets::text(label).id(format!("{name}-count"));
        if let Some(ts) = count_el.text_style_mut() {
            ts.font_size = 12.0;
            ts.color = Color::srgb8(0x6b, 0x74, 0x88, 0xff);
        }

        let mut row = crate::widgets::row(vec![
            crate::TextInput::new(cx, &find_key, "")
                .placeholder("Find")
                .id(format!("{name}-find"))
                .into(),
            crate::TextInput::new(cx, &replace_key, "")
                .placeholder("Replace with")
                .id(format!("{name}-replace"))
                .into(),
            count_el,
            crate::Button::new("Replace all")
                .on_press(apply)
                .disabled(needle.is_empty())
                .id(format!("{name}-apply"))
                .into(),
        ]);
        row.style.column_gap = Dim::px(8.0);
        row.style.align_items = Some(lumen_layout::Align::Center);
        FindReplaceBar {
            el: row,
            common: Common::default(),
        }
    }
}

impl Widget for FindReplaceBar {
    fn build(self) -> Element {
        let FindReplaceBar { mut el, common } = self;
        common.apply(&mut el);
        el
    }
}

impl_widget!(FindReplaceBar);

/// M.4: a find/replace bar operating on a [`rich_text_editor`]'s source.
/// `{name}.find` / `{name}.replace` hold the inputs; the count label shows
/// live match counts and the button rewrites every occurrence (caret resets
/// to the end; the editor's undo history keeps the previous text).
/// *(Thin shim over [`FindReplaceBar`] — the typed form is preferred.)*
pub fn find_replace_bar(cx: &BuildCx, name: &str, editor_name: &str) -> Element {
    FindReplaceBar::new(cx, name, editor_name).into()
}
