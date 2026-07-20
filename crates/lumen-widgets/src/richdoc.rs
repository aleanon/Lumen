//! M.4 — `RichDoc`: the structured rich-text model. A markdown-lite source
//! parses into blocks (headings, paragraphs, bullet/numbered list items,
//! images) of styled spans (bold, italic, links); the document renders as an
//! element tree, round-trips back to source, and supports find/replace over
//! the source. The editor (`widgets_m4::rich_text_editor`) edits the SOURCE
//! with the full `TextEditor` caret/selection/undo machinery and renders the
//! parsed preview live.
//!
//! Explicitly *planned* (not implemented): tables, spell-check, variable
//! font axes, CRDT/collaborative editing.

use crate::element::Element;
use crate::{widgets, NodeContent};
use lumen_core::semantics::Role;
use lumen_core::state::Runtime;
use lumen_core::Color;
use lumen_layout::{Dim, Display, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;
use std::rc::Rc;

/// The link-activation handler (`on_link(rt, url)`).
pub type LinkHandler = Rc<dyn Fn(&Runtime, &str)>;

/// An inline styled span.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// Text content.
    pub text: String,
    /// `**bold**`.
    pub bold: bool,
    /// `*italic*`.
    pub italic: bool,
    /// `[text](url)` target.
    pub link: Option<String>,
}

/// A block-level node.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// `#`/`##`/`###` heading (level 1–3).
    Heading(u8, Vec<Span>),
    /// A plain paragraph.
    Paragraph(Vec<Span>),
    /// `- ` bullet item.
    Bullet(Vec<Span>),
    /// `N. ` numbered item (the number is preserved).
    Numbered(u32, Vec<Span>),
    /// `![alt](src)` image reference.
    Image {
        /// Alt text (the accessible name).
        alt: String,
        /// Source reference (resolved by the app's asset layer).
        src: String,
    },
}

/// A parsed rich document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichDoc {
    /// Blocks in order.
    pub blocks: Vec<Block>,
}

impl RichDoc {
    /// Parse markdown-lite source: `#`-headings, `- ` bullets, `N. ` numbered
    /// items, `![alt](src)` image lines, and inline `**bold**` / `*italic*` /
    /// `[text](url)` spans.
    pub fn parse(src: &str) -> RichDoc {
        let mut blocks = Vec::new();
        for line in src.lines() {
            let t = line.trim_end();
            if t.trim().is_empty() {
                continue;
            }
            if let Some(rest) = t.strip_prefix("### ") {
                blocks.push(Block::Heading(3, parse_spans(rest)));
            } else if let Some(rest) = t.strip_prefix("## ") {
                blocks.push(Block::Heading(2, parse_spans(rest)));
            } else if let Some(rest) = t.strip_prefix("# ") {
                blocks.push(Block::Heading(1, parse_spans(rest)));
            } else if let Some(rest) = t.strip_prefix("- ") {
                blocks.push(Block::Bullet(parse_spans(rest)));
            } else if let Some((n, rest)) = parse_numbered(t) {
                blocks.push(Block::Numbered(n, parse_spans(rest)));
            } else if let Some((alt, src)) = parse_image_line(t.trim()) {
                blocks.push(Block::Image { alt, src });
            } else {
                blocks.push(Block::Paragraph(parse_spans(t)));
            }
        }
        RichDoc { blocks }
    }

    /// Serialize back to markdown-lite source (the round-trip contract:
    /// `parse(doc.to_source()) == doc`).
    pub fn to_source(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            match b {
                Block::Heading(l, s) => {
                    out.push_str(&"#".repeat(*l as usize));
                    out.push(' ');
                    out.push_str(&spans_to_source(s));
                }
                Block::Paragraph(s) => out.push_str(&spans_to_source(s)),
                Block::Bullet(s) => {
                    out.push_str("- ");
                    out.push_str(&spans_to_source(s));
                }
                Block::Numbered(n, s) => {
                    out.push_str(&format!("{n}. "));
                    out.push_str(&spans_to_source(s));
                }
                Block::Image { alt, src } => out.push_str(&format!("![{alt}]({src})")),
            }
            out.push('\n');
        }
        out
    }

    /// Byte offsets of every `needle` occurrence in the source.
    pub fn find(source: &str, needle: &str) -> Vec<usize> {
        if needle.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(i) = source[at..].find(needle) {
            out.push(at + i);
            at += i + needle.len();
        }
        out
    }

    /// Replace every occurrence in the source; returns (new source, count).
    pub fn replace_all(source: &str, needle: &str, with: &str) -> (String, usize) {
        if needle.is_empty() {
            return (source.to_string(), 0);
        }
        let count = Self::find(source, needle).len();
        (source.replace(needle, with), count)
    }

    /// Render as an element tree. Links get [`Role::Link`] and fire
    /// `on_link(url)`; images render as alt-labeled frames (the app resolves
    /// `src` bytes through its asset layer and can substitute real bitmaps).
    pub fn render(&self, on_link: impl Fn(&Runtime, &str) + 'static) -> Element {
        let on_link: LinkHandler = Rc::new(on_link);
        let children = self
            .blocks
            .iter()
            .map(|b| match b {
                Block::Heading(l, s) => {
                    let size = match l {
                        1 => 24.0,
                        2 => 20.0,
                        _ => 17.0,
                    };
                    spans_row(s, size, 700.0, &on_link)
                }
                Block::Paragraph(s) => spans_row(s, 15.0, 400.0, &on_link),
                Block::Bullet(s) => list_row("•", s, &on_link),
                Block::Numbered(n, s) => list_row(&format!("{n}."), s, &on_link),
                Block::Image { alt, src } => {
                    let mut ph = widgets::column(vec![widgets::text(format!("🖼 {alt}"))]);
                    ph.role = Role::Image;
                    ph.label = alt.clone();
                    ph.value = Some(src.clone());
                    ph.background = Some(Color::srgb8(0xea, 0xec, 0xf2, 0xff));
                    ph.corner_radius = 4.0;
                    ph.style.padding = lumen_layout::Edges::all(Dim::px(10.0));
                    ph
                }
            })
            .collect();
        Element {
            role: Role::Group,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Dim::px(6.0),
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        }
    }
}

fn list_row(marker: &str, spans: &[Span], on_link: &LinkHandler) -> Element {
    let mut row = widgets::row(vec![
        widgets::text(marker.to_string()),
        spans_row(spans, 15.0, 400.0, on_link),
    ]);
    row.style.column_gap = Dim::px(6.0);
    row
}

fn spans_row(spans: &[Span], size: f32, base_weight: f32, on_link: &LinkHandler) -> Element {
    let children = spans
        .iter()
        .map(|s| {
            let mut el = Element {
                role: if s.link.is_some() {
                    Role::Link
                } else {
                    Role::Text
                },
                label: s.text.clone(),
                content: NodeContent::Text(
                    s.text.clone(),
                    TextStyle {
                        font_size: size,
                        weight: if s.bold { 700.0 } else { base_weight },
                        color: if s.link.is_some() {
                            Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
                        } else if s.italic {
                            // No italic face ships (single-weight bundled
                            // font); italics render in a muted ink until
                            // variable axes land (planned).
                            Color::srgb8(0x55, 0x5b, 0x6e, 0xff)
                        } else {
                            Color::BLACK
                        },
                        ..TextStyle::default()
                    },
                ),
                ..Element::default()
            };
            if let Some(url) = &s.link {
                let url = url.clone();
                let on_link = on_link.clone();
                el.focusable = true;
                el = el.on_click(move |rt| on_link(rt, &url));
            }
            el
        })
        .collect();
    Element {
        role: Role::Group,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children,
        ..Element::default()
    }
}

fn spans_to_source(spans: &[Span]) -> String {
    let mut out = String::new();
    for s in spans {
        if let Some(url) = &s.link {
            out.push_str(&format!("[{}]({url})", s.text));
        } else if s.bold {
            out.push_str(&format!("**{}**", s.text));
        } else if s.italic {
            out.push_str(&format!("*{}*", s.text));
        } else {
            out.push_str(&s.text);
        }
    }
    out
}

fn parse_numbered(t: &str) -> Option<(u32, &str)> {
    let dot = t.find(". ")?;
    let n: u32 = t[..dot].parse().ok()?;
    Some((n, &t[dot + 2..]))
}

fn parse_image_line(t: &str) -> Option<(String, String)> {
    let rest = t.strip_prefix("![")?;
    let close = rest.find(']')?;
    let alt = rest[..close].to_string();
    let src = rest[close + 1..]
        .strip_prefix('(')?
        .strip_suffix(')')?
        .to_string();
    Some((alt, src))
}

/// Inline spans: `**bold**`, `*italic*`, `[text](url)`, plain runs.
fn parse_spans(t: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let bytes = t.as_bytes();
    let mut i = 0;
    let flush = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.push(Span {
                text: std::mem::take(plain),
                bold: false,
                italic: false,
                link: None,
            });
        }
    };
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close) = t[i..].find("](") {
                if let Some(end) = t[i + close + 2..].find(')') {
                    flush(&mut plain, &mut out);
                    out.push(Span {
                        text: t[i + 1..i + close].to_string(),
                        bold: false,
                        italic: false,
                        link: Some(t[i + close + 2..i + close + 2 + end].to_string()),
                    });
                    i += close + 2 + end + 1;
                    continue;
                }
            }
        }
        if t[i..].starts_with("**") {
            if let Some(end) = t[i + 2..].find("**") {
                flush(&mut plain, &mut out);
                out.push(Span {
                    text: t[i + 2..i + 2 + end].to_string(),
                    bold: true,
                    italic: false,
                    link: None,
                });
                i += 2 + end + 2;
                continue;
            }
        }
        if bytes[i] == b'*' {
            if let Some(end) = t[i + 1..].find('*') {
                flush(&mut plain, &mut out);
                out.push(Span {
                    text: t[i + 1..i + 1 + end].to_string(),
                    bold: false,
                    italic: true,
                    link: None,
                });
                i += 1 + end + 1;
                continue;
            }
        }
        // advance one char (respect UTF-8)
        let ch_len = t[i..].chars().next().map_or(1, char::len_utf8);
        plain.push_str(&t[i..i + ch_len]);
        i += ch_len;
    }
    flush(&mut plain, &mut out);
    if out.is_empty() {
        out.push(Span {
            text: String::new(),
            bold: false,
            italic: false,
            link: None,
        });
    }
    out
}
