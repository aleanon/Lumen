//! Minimal Markdown rendering (E8.5): a CommonMark subset — `#`/`##` headings,
//! `-` lists, fenced ``` code blocks, and inline `*emphasis*`/`` `code` `` — to
//! an `Element` tree on the styling surface. Full CommonMark (tables, nested
//! lists, links/images) layers on this block model.

use crate::widgets;
use crate::Element;
use lumen_core::Color;
use lumen_layout::{Display, FlexDirection, LayoutStyle};
use lumen_text::TextStyle;

/// Render a Markdown document to a column of block elements.
pub fn render(src: &str) -> Element {
    let mut blocks = Vec::new();
    let mut in_code = false;
    let mut code = String::new();
    for line in src.lines() {
        if line.trim_start().starts_with("```") {
            if in_code {
                blocks.push(code_block(&code));
                code.clear();
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            code.push_str(line);
            code.push('\n');
            continue;
        }
        let t = line.trim_end();
        if let Some(h) = t.strip_prefix("## ") {
            blocks.push(heading(h, 18.0));
        } else if let Some(h) = t.strip_prefix("# ") {
            blocks.push(heading(h, 24.0));
        } else if let Some(li) = t.strip_prefix("- ") {
            blocks.push(inline(&format!("•  {li}")));
        } else if !t.is_empty() {
            blocks.push(inline(t));
        }
    }
    widgets::column(blocks).id("markdown")
}

fn heading(text: &str, size: f32) -> Element {
    Element {
        role: lumen_core::semantics::Role::Text,
        label: text.to_string(),
        text: Some((
            text.to_string(),
            TextStyle {
                font_size: size,
                weight: 400.0,
                color: Color::BLACK,
                line_height: None,
                letter_spacing: 0.0,
            },
        )),
        ..Element::default()
    }
}

fn code_block(code: &str) -> Element {
    Element {
        role: lumen_core::semantics::Role::Text,
        label: code.trim_end().to_string(),
        background: Some(Color::srgb8(0xf2, 0xf2, 0xf4, 0xff)),
        text: Some((
            code.trim_end().to_string(),
            TextStyle {
                font_size: 13.0,
                weight: 400.0,
                color: Color::srgb8(0x33, 0x33, 0x33, 0xff),
                line_height: None,
                letter_spacing: 0.0,
            },
        )),
        ..Element::default()
    }
}

/// Parse inline `*emphasis*` and `` `code` `` into a row of styled runs.
fn inline(text: &str) -> Element {
    let mut runs: Vec<Element> = Vec::new();
    let mut cur = String::new();
    let mut emph = false;
    let mut codespan = false;
    let flush = |cur: &mut String, emph: bool, codespan: bool, runs: &mut Vec<Element>| {
        if cur.is_empty() {
            return;
        }
        let color = if emph {
            Color::srgb8(0xc0, 0x39, 0x2b, 0xff)
        } else if codespan {
            Color::srgb8(0x6a, 0x3d, 0x9a, 0xff)
        } else {
            Color::BLACK
        };
        runs.push(Element {
            role: lumen_core::semantics::Role::Text,
            label: cur.clone(),
            text: Some((
                std::mem::take(cur),
                TextStyle {
                    font_size: 14.0,
                    weight: 400.0,
                    color,
                    line_height: None,
                    letter_spacing: 0.0,
                },
            )),
            ..Element::default()
        });
    };
    for ch in text.chars() {
        match ch {
            '*' => {
                flush(&mut cur, emph, codespan, &mut runs);
                emph = !emph;
            }
            '`' => {
                flush(&mut cur, emph, codespan, &mut runs);
                codespan = !codespan;
            }
            _ => cur.push(ch),
        }
    }
    flush(&mut cur, emph, codespan, &mut runs);
    Element {
        role: lumen_core::semantics::Role::Group,
        label: text.replace(['*', '`'], ""),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            ..LayoutStyle::default()
        },
        children: runs,
        ..Element::default()
    }
}
