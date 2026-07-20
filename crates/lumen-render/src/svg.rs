//! SVG rendering (T6.2, completed in plan M.2): a dependency-free parser for
//! the useful core of static SVG — nested `<g>` groups with inheritance,
//! `transform` (translate/scale/rotate/matrix, composed and flattened into
//! the geometry), linear/radial gradients from `<defs>`, rect-shaped
//! `clip-path`, fill + stroke with opacity, the full path grammar
//! (`MmLlHhVvCcSsQqTtAaZz`), and `<text>` through a caller-supplied shaper
//! (the text stack lives ABOVE this crate — `lumen-widgets` passes a
//! `TextEngine`-backed closure; see `parse_with_text`).
//!
//! Deliberately NOT usvg: adopting it would ship a second font stack
//! (fontdb + rustybuzz beside parley) for text we can already shape, and a
//! dependency tree for features this covers. Unsupported (documented, ADR-M1
//! addendum): filters, masks, patterns, non-rect clip paths (skipped),
//! `use`/symbol, animations/Lottie (post-2.0), external refs.

use crate::cpu;
use crate::display_list::{
    Brush, CornerRadii, DisplayList, DrawCmd, FillOrStroke, GlyphImage, GlyphRun, GlyphRunId,
    GradientStop, RoundedRect, SpreadMode,
};
use crate::image::RgbaImage;
use kurbo::{Affine, BezPath, Circle, Ellipse, PathEl, Point, Rect, Shape, Vec2};
use lumen_core::Color;
use std::collections::HashMap;

/// A `<text>` element the parser met: the caller shapes it (the text stack is
/// above this crate) and returns the run + its rasterized glyphs.
pub struct TextSpec<'a> {
    /// The text content.
    pub text: &'a str,
    /// Anchor (the SVG `x`/`y` baseline point), already transformed.
    pub pos: Point,
    /// `font-size` (px), scaled by the current transform's average scale.
    pub size: f64,
    /// Fill color.
    pub color: Color,
}

/// Shaper callback: `TextSpec` in, `(run, glyph images, bounds)` out.
pub type TextShaper<'a> = &'a mut dyn FnMut(&TextSpec) -> Option<(GlyphRun, Vec<GlyphImage>, Rect)>;

/// Parse an SVG document into a display list (no text shaping — `<text>`
/// elements are skipped; use [`parse_with_text`]).
pub fn parse(src: &str) -> DisplayList {
    parse_with_text(src, &mut |_| None)
}

/// Parse an SVG document, shaping `<text>` through `shaper`.
pub fn parse_with_text(src: &str, shaper: TextShaper) -> DisplayList {
    let root = parse_tree(src);
    let mut defs = HashMap::new();
    collect_defs(&root, &mut defs);
    let mut dl = DisplayList::new();
    let ctx = Ctx {
        transform: Affine::IDENTITY,
        fill: Some(Paint::Solid(Color::BLACK)),
        stroke: None,
        stroke_width: 1.0,
        opacity: 1.0,
    };
    for child in &root.children {
        walk(child, &ctx, &defs, &mut dl, shaper);
    }
    dl
}

/// Render an SVG document on the deterministic CPU renderer.
pub fn render(src: &str, width: u32, height: u32, background: Color) -> RgbaImage {
    cpu::render(&parse(src), width, height, background)
}

// --- mini-DOM ----------------------------------------------------------------

struct Node {
    name: String,
    tag: String, // raw attribute text
    text: String,
    children: Vec<Node>,
}

/// A tiny fault-tolerant XML tree parser (elements, attributes as raw tag
/// text, text content). Comments/PI/doctype skipped.
fn parse_tree(src: &str) -> Node {
    let mut root = Node {
        name: "svg".into(),
        tag: String::new(),
        text: String::new(),
        children: Vec::new(),
    };
    let mut stack: Vec<Node> = Vec::new();
    let mut rest = src;
    while let Some(open) = rest.find('<') {
        let text_before = &rest[..open];
        if !text_before.trim().is_empty() {
            if let Some(top) = stack.last_mut() {
                top.text.push_str(text_before.trim());
            }
        }
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        let tag = rest[open + 1..open + close].trim().to_string();
        rest = &rest[open + close + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if tag.strip_prefix('/').is_some() {
            // closing tag: pop and attach
            if let Some(done) = stack.pop() {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => {
                        if done.name == "svg" {
                            root = done;
                        } else {
                            root.children.push(done);
                        }
                    }
                }
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let body = tag.trim_end_matches('/').trim();
        let name = body
            .split([' ', '\t', '\n'])
            .next()
            .unwrap_or("")
            .to_string();
        let node = Node {
            name,
            tag: body.to_string(),
            text: String::new(),
            children: Vec::new(),
        };
        if self_closing {
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => root.children.push(node),
            }
        } else {
            stack.push(node);
        }
    }
    // Unclosed leftovers attach upward.
    while let Some(done) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(done),
            None => {
                if done.name == "svg" {
                    root = done;
                } else {
                    root.children.push(done);
                }
            }
        }
    }
    root
}

// --- defs: gradients + clip paths ---------------------------------------------

enum Def {
    Linear {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    },
    Radial {
        center: Point,
        radius: f64,
        stops: Vec<GradientStop>,
    },
    /// Rect-shaped clip (the supported clip subset).
    ClipRect(Rect),
}

fn collect_defs(node: &Node, defs: &mut HashMap<String, Def>) {
    for child in &node.children {
        match child.name.as_str() {
            "linearGradient" => {
                if let Some(id) = attr(&child.tag, "id") {
                    defs.insert(
                        id,
                        Def::Linear {
                            start: Point::new(num(&child.tag, "x1"), num(&child.tag, "y1")),
                            end: Point::new(num(&child.tag, "x2"), num(&child.tag, "y2")),
                            stops: stops(child),
                        },
                    );
                }
            }
            "radialGradient" => {
                if let Some(id) = attr(&child.tag, "id") {
                    defs.insert(
                        id,
                        Def::Radial {
                            center: Point::new(num(&child.tag, "cx"), num(&child.tag, "cy")),
                            radius: num(&child.tag, "r"),
                            stops: stops(child),
                        },
                    );
                }
            }
            "clipPath" => {
                if let (Some(id), Some(rect)) = (
                    attr(&child.tag, "id"),
                    child.children.iter().find(|c| c.name == "rect"),
                ) {
                    let x = num(&rect.tag, "x");
                    let y = num(&rect.tag, "y");
                    defs.insert(
                        id,
                        Def::ClipRect(Rect::new(
                            x,
                            y,
                            x + num(&rect.tag, "width"),
                            y + num(&rect.tag, "height"),
                        )),
                    );
                }
            }
            _ => collect_defs(child, defs),
        }
    }
}

fn stops(node: &Node) -> Vec<GradientStop> {
    node.children
        .iter()
        .filter(|c| c.name == "stop")
        .map(|c| {
            let offset = attr(&c.tag, "offset")
                .map(|o| {
                    let o = o.trim();
                    if let Some(p) = o.strip_suffix('%') {
                        p.parse::<f32>().unwrap_or(0.0) / 100.0
                    } else {
                        o.parse().unwrap_or(0.0)
                    }
                })
                .unwrap_or(0.0);
            let mut color = attr(&c.tag, "stop-color")
                .and_then(|v| Color::from_hex(&v).ok())
                .unwrap_or(Color::BLACK);
            if let Some(op) = attr(&c.tag, "stop-opacity").and_then(|v| v.parse::<f32>().ok()) {
                color.a *= op.clamp(0.0, 1.0);
            }
            GradientStop { offset, color }
        })
        .collect()
}

// --- the walk ------------------------------------------------------------------

#[derive(Clone)]
enum Paint {
    Solid(Color),
    Url(String),
}

#[derive(Clone)]
struct Ctx {
    transform: Affine,
    fill: Option<Paint>,
    stroke: Option<Color>,
    stroke_width: f64,
    opacity: f32,
}

fn walk(
    node: &Node,
    parent: &Ctx,
    defs: &HashMap<String, Def>,
    dl: &mut DisplayList,
    shaper: TextShaper,
) {
    if matches!(
        node.name.as_str(),
        "defs" | "linearGradient" | "radialGradient" | "clipPath" | "style" | "metadata" | "title"
    ) {
        return;
    }
    let mut ctx = parent.clone();
    if let Some(t) = attr(&node.tag, "transform") {
        ctx.transform *= parse_transform(&t);
    }
    if let Some(f) = attr(&node.tag, "fill") {
        ctx.fill = parse_paint(&f);
    }
    if let Some(s) = attr(&node.tag, "stroke") {
        ctx.stroke = if s == "none" {
            None
        } else {
            Color::from_hex(&s).ok()
        };
    }
    if let Some(w) = attr(&node.tag, "stroke-width").and_then(|v| v.parse().ok()) {
        ctx.stroke_width = w;
    }
    if let Some(o) = attr(&node.tag, "opacity").and_then(|v| v.parse::<f32>().ok()) {
        ctx.opacity *= o.clamp(0.0, 1.0);
    }
    if let Some(fo) = attr(&node.tag, "fill-opacity").and_then(|v| v.parse::<f32>().ok()) {
        // Applied to the resolved fill color (solid fills only).
        if let Some(Paint::Solid(mut c)) = ctx.fill.clone() {
            c.a *= fo.clamp(0.0, 1.0);
            ctx.fill = Some(Paint::Solid(c));
        }
    }

    // clip-path="url(#id)" (rect subset) → a clipped layer around the subtree.
    let clip = attr(&node.tag, "clip-path")
        .and_then(|v| url_ref(&v))
        .and_then(|id| match defs.get(&id) {
            Some(Def::ClipRect(r)) => Some(*r),
            _ => None,
        });
    let group_opacity = attr(&node.tag, "opacity").is_some() && ctx.opacity < 1.0;
    let needs_layer = clip.is_some() || group_opacity;
    if needs_layer {
        dl.push(DrawCmd::PushLayer {
            clip: clip.map(|r| RoundedRect {
                rect: ctx.transform.transform_rect_bbox(r),
                radii: CornerRadii::all(0.0),
            }),
            opacity: if group_opacity {
                ctx.opacity / parent.opacity.max(f32::EPSILON)
            } else {
                1.0
            },
            transform: Affine::IDENTITY,
            blend: crate::display_list::BlendMode::SourceOver,
        });
    }

    match node.name.as_str() {
        "g" | "svg" | "a" => {
            for child in &node.children {
                walk(child, &ctx, defs, dl, shaper);
            }
        }
        "rect" => {
            let x = num(&node.tag, "x");
            let y = num(&node.tag, "y");
            let r = Rect::new(
                x,
                y,
                x + num(&node.tag, "width"),
                y + num(&node.tag, "height"),
            );
            emit(dl, &ctx, defs, r.to_path(0.1));
        }
        "circle" => {
            let c = Circle::new(
                Point::new(num(&node.tag, "cx"), num(&node.tag, "cy")),
                num(&node.tag, "r"),
            );
            emit(dl, &ctx, defs, c.to_path(0.1));
        }
        "ellipse" => {
            let e = Ellipse::new(
                Point::new(num(&node.tag, "cx"), num(&node.tag, "cy")),
                Vec2::new(num(&node.tag, "rx"), num(&node.tag, "ry")),
                0.0,
            );
            emit(dl, &ctx, defs, e.to_path(0.1));
        }
        "line" => {
            let mut p = BezPath::new();
            p.move_to(Point::new(num(&node.tag, "x1"), num(&node.tag, "y1")));
            p.line_to(Point::new(num(&node.tag, "x2"), num(&node.tag, "y2")));
            emit_stroke_only(dl, &ctx, p);
        }
        "polyline" | "polygon" => {
            if let Some(pts) = attr(&node.tag, "points") {
                let nums: Vec<f64> = pts
                    .split([',', ' ', '\t', '\n'])
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                let mut p = BezPath::new();
                for (i, xy) in nums.chunks_exact(2).enumerate() {
                    let pt = Point::new(xy[0], xy[1]);
                    if i == 0 {
                        p.move_to(pt);
                    } else {
                        p.line_to(pt);
                    }
                }
                if node.name == "polygon" {
                    p.close_path();
                    emit(dl, &ctx, defs, p);
                } else {
                    emit_stroke_only(dl, &ctx, p);
                }
            }
        }
        "path" => {
            if let Some(d) = attr(&node.tag, "d") {
                emit(dl, &ctx, defs, parse_path(&d));
            }
        }
        "text" => {
            let (sx, sy) = scale_of(ctx.transform);
            let spec = TextSpec {
                text: node.text.trim(),
                pos: ctx.transform * Point::new(num(&node.tag, "x"), num(&node.tag, "y")),
                size: attr(&node.tag, "font-size")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(16.0)
                    * ((sx + sy) / 2.0),
                color: match &ctx.fill {
                    Some(Paint::Solid(c)) => *c,
                    _ => Color::BLACK,
                },
            };
            if !spec.text.is_empty() {
                if let Some((mut run, images, rect)) = shaper(&spec) {
                    let base = dl.glyph_images.len() as u32;
                    for g in &mut run.glyphs {
                        g.image += base;
                    }
                    dl.glyph_images.extend(images);
                    dl.runs.push(run);
                    dl.push(DrawCmd::GlyphRun {
                        run: GlyphRunId((dl.runs.len() - 1) as u32),
                        brush: Brush::Solid(spec.color),
                        rect,
                    });
                }
            }
        }
        _ => {
            for child in &node.children {
                walk(child, &ctx, defs, dl, shaper);
            }
        }
    }

    if needs_layer {
        dl.push(DrawCmd::PopLayer);
    }
}

/// Fill (solid or gradient ref) + optional stroke for a path, transformed.
fn emit(dl: &mut DisplayList, ctx: &Ctx, defs: &HashMap<String, Def>, mut path: BezPath) {
    path.apply_affine(ctx.transform);
    match &ctx.fill {
        Some(Paint::Solid(c)) if c.a > 0.0 => {
            let mut c = *c;
            c.a *= ctx.opacity;
            dl.push(DrawCmd::Path {
                path: path.clone(),
                brush: Brush::Solid(c),
                style: FillOrStroke::Fill,
            });
        }
        Some(Paint::Url(id)) => {
            if let Some(brush) = gradient_brush(defs, id, ctx.transform) {
                dl.push(DrawCmd::Path {
                    path: path.clone(),
                    brush,
                    style: FillOrStroke::Fill,
                });
            }
        }
        _ => {}
    }
    if let Some(c) = ctx.stroke {
        let (sx, sy) = scale_of(ctx.transform);
        dl.push(DrawCmd::Path {
            path,
            brush: Brush::Solid(c),
            style: FillOrStroke::Stroke {
                width: ctx.stroke_width * ((sx + sy) / 2.0),
            },
        });
    }
}

fn emit_stroke_only(dl: &mut DisplayList, ctx: &Ctx, mut path: BezPath) {
    path.apply_affine(ctx.transform);
    let color = ctx.stroke.unwrap_or(match ctx.fill {
        Some(Paint::Solid(c)) => c,
        _ => Color::BLACK,
    });
    let (sx, sy) = scale_of(ctx.transform);
    dl.push(DrawCmd::Path {
        path,
        brush: Brush::Solid(color),
        style: FillOrStroke::Stroke {
            width: ctx.stroke_width * ((sx + sy) / 2.0),
        },
    });
}

fn gradient_brush(defs: &HashMap<String, Def>, id: &str, t: Affine) -> Option<Brush> {
    match defs.get(id)? {
        Def::Linear { start, end, stops } => Some(Brush::LinearGradient {
            start: t * *start,
            end: t * *end,
            stops: stops.clone(),
            spread: SpreadMode::Pad,
        }),
        Def::Radial {
            center,
            radius,
            stops,
        } => {
            let (sx, sy) = scale_of(t);
            Some(Brush::RadialGradient {
                center: t * *center,
                radius: radius * ((sx + sy) / 2.0),
                stops: stops.clone(),
                spread: SpreadMode::Pad,
            })
        }
        Def::ClipRect(_) => None,
    }
}

// --- attribute helpers ---------------------------------------------------------

fn attr(tag: &str, name: &str) -> Option<String> {
    // `name="value"` with a word boundary before `name`.
    let mut search = 0;
    let key = format!("{name}=\"");
    while let Some(rel) = tag[search..].find(&key) {
        let at = search + rel;
        let boundary_ok = at == 0
            || tag[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        let start = at + key.len();
        let end = tag[start..].find('"')? + start;
        if boundary_ok {
            return Some(tag[start..end].to_string());
        }
        search = end;
    }
    None
}

fn num(tag: &str, name: &str) -> f64 {
    attr(tag, name).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

fn url_ref(v: &str) -> Option<String> {
    let inner = v.trim().strip_prefix("url(#")?.strip_suffix(')')?;
    Some(inner.to_string())
}

fn parse_paint(v: &str) -> Option<Paint> {
    let v = v.trim();
    if v == "none" {
        return None;
    }
    if let Some(id) = url_ref(v) {
        return Some(Paint::Url(id));
    }
    Color::from_hex(v).ok().map(Paint::Solid)
}

/// `translate/scale/rotate/matrix` composed left-to-right.
fn parse_transform(v: &str) -> Affine {
    let mut out = Affine::IDENTITY;
    let mut rest = v;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let args: Vec<f64> = rest[open + 1..open + close]
            .split([',', ' ', '\t', '\n'])
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let t = match (name, args.as_slice()) {
            ("translate", [x]) => Affine::translate((*x, 0.0)),
            ("translate", [x, y, ..]) => Affine::translate((*x, *y)),
            ("scale", [s]) => Affine::scale(*s),
            ("scale", [x, y, ..]) => Affine::scale_non_uniform(*x, *y),
            ("rotate", [a]) => Affine::rotate(a.to_radians()),
            ("rotate", [a, cx, cy, ..]) => {
                Affine::translate((*cx, *cy))
                    * Affine::rotate(a.to_radians())
                    * Affine::translate((-*cx, -*cy))
            }
            ("matrix", [a, b, c, d, e, f, ..]) => Affine::new([*a, *b, *c, *d, *e, *f]),
            _ => Affine::IDENTITY,
        };
        out *= t;
        rest = &rest[open + close + 1..];
    }
    out
}

fn scale_of(t: Affine) -> (f64, f64) {
    let c = t.as_coeffs();
    (
        (c[0] * c[0] + c[1] * c[1]).sqrt(),
        (c[2] * c[2] + c[3] * c[3]).sqrt(),
    )
}

// --- path grammar ----------------------------------------------------------------

/// Parse an SVG path `d` attribute: `MmLlHhVvCcSsQqTtAaZz`, absolute and
/// relative, with implicit command repetition. Arcs convert to cubics via
/// kurbo.
fn parse_path(d: &str) -> BezPath {
    let mut path = BezPath::new();
    let mut nums: Vec<f64> = Vec::new();
    let mut cmd = ' ';
    let mut cur = Point::ZERO;
    let mut start = Point::ZERO;
    let mut last_ctrl: Option<Point> = None; // for S/T reflection

    fn take(n: &mut Vec<f64>, k: usize) -> Option<Vec<f64>> {
        if n.len() >= k {
            Some(n.drain(..k).collect())
        } else {
            n.clear();
            None
        }
    }

    fn apply(
        path: &mut BezPath,
        cmd: char,
        nums: &mut Vec<f64>,
        cur: &mut Point,
        start: &mut Point,
        last_ctrl: &mut Option<Point>,
    ) {
        loop {
            let rel = cmd.is_ascii_lowercase();
            let o = if rel {
                Vec2::new(cur.x, cur.y)
            } else {
                Vec2::ZERO
            };
            match cmd.to_ascii_uppercase() {
                'M' => {
                    let Some(a) = take(nums, 2) else { break };
                    *cur = Point::new(a[0] + o.x, a[1] + o.y);
                    *start = *cur;
                    path.push(PathEl::MoveTo(*cur));
                    *last_ctrl = None;
                }
                'L' => {
                    let Some(a) = take(nums, 2) else { break };
                    *cur = Point::new(a[0] + o.x, a[1] + o.y);
                    path.push(PathEl::LineTo(*cur));
                    *last_ctrl = None;
                }
                'H' => {
                    let Some(a) = take(nums, 1) else { break };
                    *cur = Point::new(a[0] + o.x, cur.y);
                    path.push(PathEl::LineTo(*cur));
                    *last_ctrl = None;
                }
                'V' => {
                    let Some(a) = take(nums, 1) else { break };
                    *cur = Point::new(cur.x, a[0] + o.y);
                    path.push(PathEl::LineTo(*cur));
                    *last_ctrl = None;
                }
                'C' => {
                    let Some(a) = take(nums, 6) else { break };
                    let c1 = Point::new(a[0] + o.x, a[1] + o.y);
                    let c2 = Point::new(a[2] + o.x, a[3] + o.y);
                    *cur = Point::new(a[4] + o.x, a[5] + o.y);
                    path.push(PathEl::CurveTo(c1, c2, *cur));
                    *last_ctrl = Some(c2);
                }
                'S' => {
                    let Some(a) = take(nums, 4) else { break };
                    let c1 = last_ctrl.map_or(*cur, |c| *cur + (*cur - c));
                    let c2 = Point::new(a[0] + o.x, a[1] + o.y);
                    *cur = Point::new(a[2] + o.x, a[3] + o.y);
                    path.push(PathEl::CurveTo(c1, c2, *cur));
                    *last_ctrl = Some(c2);
                }
                'Q' => {
                    let Some(a) = take(nums, 4) else { break };
                    let c = Point::new(a[0] + o.x, a[1] + o.y);
                    *cur = Point::new(a[2] + o.x, a[3] + o.y);
                    path.push(PathEl::QuadTo(c, *cur));
                    *last_ctrl = Some(c);
                }
                'T' => {
                    let Some(a) = take(nums, 2) else { break };
                    let c = last_ctrl.map_or(*cur, |c| *cur + (*cur - c));
                    *cur = Point::new(a[0] + o.x, a[1] + o.y);
                    path.push(PathEl::QuadTo(c, *cur));
                    *last_ctrl = Some(c);
                }
                'A' => {
                    let Some(a) = take(nums, 7) else { break };
                    let to = Point::new(a[5] + o.x, a[6] + o.y);
                    let arc = kurbo::SvgArc {
                        from: *cur,
                        to,
                        radii: Vec2::new(a[0].abs(), a[1].abs()),
                        x_rotation: a[2].to_radians(),
                        large_arc: a[3] != 0.0,
                        sweep: a[4] != 0.0,
                    };
                    match kurbo::Arc::from_svg_arc(&arc) {
                        Some(arc) => {
                            arc.to_cubic_beziers(0.1, |c1, c2, p| {
                                path.push(PathEl::CurveTo(c1, c2, p));
                            });
                        }
                        None => path.push(PathEl::LineTo(to)), // degenerate
                    }
                    *cur = to;
                    *last_ctrl = None;
                }
                'Z' => {
                    path.push(PathEl::ClosePath);
                    *cur = *start;
                    *last_ctrl = None;
                    break;
                }
                _ => break,
            }
            if nums.is_empty() {
                break;
            }
        }
        nums.clear();
    }

    let mut token = String::new();
    fn flush_token(nums: &mut Vec<f64>, token: &mut String) {
        if !token.is_empty() {
            if let Ok(v) = token.parse() {
                nums.push(v);
            }
            token.clear();
        }
    }
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush_token(&mut nums, &mut token);
            if cmd != ' ' {
                apply(
                    &mut path,
                    cmd,
                    &mut nums,
                    &mut cur,
                    &mut start,
                    &mut last_ctrl,
                );
            }
            cmd = ch;
        } else if ch == ',' || ch.is_whitespace() {
            flush_token(&mut nums, &mut token);
        } else if ch == '-' && !token.is_empty() && !token.ends_with(['e', 'E']) {
            // "10-20" is two numbers
            flush_token(&mut nums, &mut token);
            token.push(ch);
        } else {
            token.push(ch);
        }
    }
    flush_token(&mut nums, &mut token);
    if cmd != ' ' {
        apply(
            &mut path,
            cmd,
            &mut nums,
            &mut cur,
            &mut start,
            &mut last_ctrl,
        );
    }
    path
}
