//! Minimal SVG rendering (T6.2): parse a small subset of SVG (`rect`, `circle`,
//! `path` with `M`/`L`/`C`/`Z`, solid `fill`) into the display list and render
//! it on the deterministic CPU renderer — so vector assets get exact goldens.
//!
//! This is the verifiable core; full SVG (gradients, transforms, text, clips),
//! Lottie, and the extra image codecs (jpeg/webp/avif) are larger asset-pipeline
//! work tracked separately.

use crate::cpu;
use crate::display_list::{Brush, DisplayList, DrawCmd, FillOrStroke};
use crate::image::RgbaImage;
use kurbo::{BezPath, Circle, PathEl, Point, Rect, Shape};
use lumen_core::Color;

/// Parse a small SVG document into a display list.
pub fn parse(src: &str) -> DisplayList {
    let mut dl = DisplayList::new();
    for tag in elements(src) {
        let name = tag.split([' ', '\t', '\n', '/', '>']).next().unwrap_or("");
        let fill = attr(&tag, "fill")
            .and_then(|f| Color::from_hex(&f).ok())
            .unwrap_or(Color::BLACK);
        match name {
            "rect" => {
                let x = num(&tag, "x");
                let y = num(&tag, "y");
                let w = num(&tag, "width");
                let h = num(&tag, "height");
                dl.push(DrawCmd::Rect {
                    rect: Rect::new(x, y, x + w, y + h),
                    brush: Brush::Solid(fill),
                    radii: crate::display_list::CornerRadii::all(0.0),
                    border: None,
                });
            }
            "circle" => {
                let c = Circle::new(Point::new(num(&tag, "cx"), num(&tag, "cy")), num(&tag, "r"));
                dl.push(DrawCmd::Path {
                    path: c.to_path(0.1),
                    brush: Brush::Solid(fill),
                    style: FillOrStroke::Fill,
                });
            }
            "path" => {
                if let Some(d) = attr(&tag, "d") {
                    dl.push(DrawCmd::Path {
                        path: parse_path(&d),
                        brush: Brush::Solid(fill),
                        style: FillOrStroke::Fill,
                    });
                }
            }
            _ => {}
        }
    }
    dl
}

/// Render an SVG document to a `width`×`height` image over `background`.
pub fn render(src: &str, width: u32, height: u32, background: Color) -> RgbaImage {
    cpu::render(&parse(src), width, height, background)
}

/// Iterate element open-tags (`<name …>` / `<name …/>`), excluding the root
/// `<svg>` and closing tags.
fn elements(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        let tag = rest[open + 1..open + close].trim().to_string();
        rest = &rest[open + close + 1..];
        if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let name = tag.split([' ', '\t', '\n', '/']).next().unwrap_or("");
        if name != "svg" && !name.is_empty() {
            out.push(tag);
        }
    }
    out
}

/// Extract `name="value"` from a tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn num(tag: &str, name: &str) -> f64 {
    attr(tag, name).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// Parse an SVG path `d` attribute supporting absolute `M`, `L`, `C`, `Z`.
fn parse_path(d: &str) -> BezPath {
    let mut path = BezPath::new();
    let mut nums: Vec<f64> = Vec::new();
    let mut cmd = ' ';
    let flush = |path: &mut BezPath, cmd: char, n: &mut Vec<f64>| {
        match cmd {
            'M' if n.len() >= 2 => path.push(PathEl::MoveTo(Point::new(n[0], n[1]))),
            'L' if n.len() >= 2 => path.push(PathEl::LineTo(Point::new(n[0], n[1]))),
            'C' if n.len() >= 6 => path.push(PathEl::CurveTo(
                Point::new(n[0], n[1]),
                Point::new(n[2], n[3]),
                Point::new(n[4], n[5]),
            )),
            'Z' => path.push(PathEl::ClosePath),
            _ => {}
        }
        n.clear();
    };
    let mut token = String::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            if !token.is_empty() {
                if let Ok(v) = token.parse() {
                    nums.push(v);
                }
                token.clear();
            }
            if cmd != ' ' {
                flush(&mut path, cmd, &mut nums);
            }
            cmd = ch;
        } else if ch == ',' || ch.is_whitespace() {
            if !token.is_empty() {
                if let Ok(v) = token.parse() {
                    nums.push(v);
                }
                token.clear();
            }
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        if let Ok(v) = token.parse() {
            nums.push(v);
        }
    }
    if cmd != ' ' {
        flush(&mut path, cmd, &mut nums);
    }
    path
}
