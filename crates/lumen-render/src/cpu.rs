//! The CPU reference renderer (tiny-skia), per ADR-002.
//!
//! Deterministic and headless: given identical input it produces byte-identical
//! output (no time-dependent dithering, fixed iteration order), which is what
//! makes golden-image testing trustworthy. Layers are composited via an
//! offscreen-pixmap stack; gradients are baked into many stops interpolated in
//! Oklab (ADR-017); conic gradients and the shader fallback are sampled
//! directly since tiny-skia has no native sweep gradient.
//!
//! Damage rendering ([`render_damage`]) recomputes in the full coordinate space
//! and returns the dirty crop, which is byte-identical to the full frame there;
//! see that function for why a translated sub-canvas cannot be (f32 AA).

use crate::display_list::*;
use crate::image::RgbaImage;
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use lumen_core::Color;
use tiny_skia::{
    FillRule, GradientStop as TsStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap,
    PixmapPaint, RadialGradient, Shader, Stroke, Transform,
};

/// Number of baked sub-stops per gradient segment (Oklab interpolation).
const GRADIENT_BAKE_STEPS: usize = 24;

/// Render `list` into a `width`×`height` image over `background`.
///
/// This is the renderer of record: the result is bit-deterministic.
pub fn render(list: &DisplayList, width: u32, height: u32, background: Color) -> RgbaImage {
    let mut r = Renderer::new(width, height, 0.0, 0.0, &list.images);
    r.run(list, background);
    r.finish()
}

/// Like [`render`], but rasterizes a display list authored in *logical* pixels
/// at a *physical* resolution `scale`× larger (HiDPI). `width`/`height` are
/// physical pixels; every command is scaled by `scale`. `scale == 1.0` is
/// byte-identical to [`render`].
pub fn render_scaled(
    list: &DisplayList,
    width: u32,
    height: u32,
    scale: f64,
    background: Color,
) -> RgbaImage {
    let mut r = Renderer::new(width, height, 0.0, 0.0, &list.images);
    r.base = Transform::from_scale(scale as f32, scale as f32);
    r.run(list, background);
    r.finish()
}

/// Re-render only the `dirty` rectangle, returning a `dirty`-sized image that is
/// byte-identical to [`render`]'s output cropped to `dirty`.
///
/// The CPU reference renderer recomputes in the full coordinate space and
/// exposes the dirty crop. This is deliberate: tiny-skia's anti-aliased
/// coverage is computed in f32 and is *not* translation-invariant, so rendering
/// a translated sub-canvas would differ from the full frame by ±1 on AA edges.
/// True partial-pixel redraw (skipping clean pixels) is a GPU-backend
/// optimization (T0.11); on CPU, exactness is the contract (02 §7).
pub fn render_damage(
    list: &DisplayList,
    width: u32,
    height: u32,
    background: Color,
    dirty: Rect,
) -> RgbaImage {
    let full = render(list, width, height, background);
    let x = dirty.x0.floor().max(0.0) as u32;
    let y = dirty.y0.floor().max(0.0) as u32;
    let w = (dirty.x1.ceil() - x as f64).max(0.0) as u32;
    let h = (dirty.y1.ceil() - y as f64).max(0.0) as u32;
    full.crop(x, y, w, h)
}

struct LayerParams {
    clip: Option<RoundedRect>,
    opacity: f32,
    transform: Affine,
    blend: BlendMode,
}

struct Renderer<'a> {
    width: u32,
    height: u32,
    /// Window→canvas transform (integer translation for damage, identity else).
    base: Transform,
    /// Window-space origin of the canvas (the dirty rect's top-left).
    origin: (f64, f64),
    images: &'a [RgbaImage],
    layers: Vec<Pixmap>,
    params: Vec<LayerParams>,
}

impl<'a> Renderer<'a> {
    fn new(width: u32, height: u32, ox: f64, oy: f64, images: &'a [RgbaImage]) -> Renderer<'a> {
        Renderer {
            width,
            height,
            base: Transform::from_translate(-ox as f32, -oy as f32),
            origin: (ox, oy),
            images,
            layers: Vec::new(),
            params: Vec::new(),
        }
    }

    fn run(&mut self, list: &DisplayList, background: Color) {
        let mut base = Pixmap::new(self.width.max(1), self.height.max(1)).expect("valid size");
        base.fill(ts_color(background));
        self.layers.push(base);
        for cmd in &list.cmds {
            self.exec(cmd);
        }
        while self.layers.len() > 1 {
            self.pop_layer();
        }
    }

    fn finish(mut self) -> RgbaImage {
        RgbaImage::from_pixmap(&self.layers.pop().expect("base layer"))
    }

    fn top(&mut self) -> &mut Pixmap {
        self.layers.last_mut().expect("at least the base layer")
    }

    fn exec(&mut self, cmd: &DrawCmd) {
        match cmd {
            DrawCmd::Rect {
                rect,
                brush,
                radii,
                border,
            } => self.draw_rect(*rect, brush, *radii, border.as_ref()),
            DrawCmd::Path { path, brush, style } => self.draw_path(path, brush, *style),
            DrawCmd::Image {
                id,
                src_rect,
                dst_rect,
                quality,
            } => self.draw_image(*id, *src_rect, *dst_rect, *quality),
            DrawCmd::GlyphRun { .. } => { /* text stack lands in T0.6 */ }
            DrawCmd::PushLayer {
                clip,
                opacity,
                transform,
                blend,
            } => self.push_layer(*clip, *opacity, *transform, *blend),
            DrawCmd::PopLayer => self.pop_layer(),
            DrawCmd::Shader { rect, uniforms, .. } => self.draw_rect(
                *rect,
                &Brush::Solid(uniforms.fallback),
                CornerRadii::ZERO,
                None,
            ),
            DrawCmd::BackdropFilter {
                rect,
                radii,
                blur,
                saturate,
            } => self.backdrop_filter(*rect, *radii, *blur, *saturate),
        }
    }

    /// Glass `backdrop-filter`: snapshot the painted backdrop under `rect`, blur
    /// (+ optionally saturate) it, and composite it back clipped to the rounded
    /// rect — so the node's translucent fill (drawn next) reads as frosted glass.
    fn backdrop_filter(&mut self, rect: Rect, radii: CornerRadii, blur: f32, saturate: f32) {
        if blur <= 0.0 && (saturate - 1.0).abs() < 1e-3 {
            return;
        }
        // Map the region to physical pixels (HiDPI scale / damage origin).
        let mut pts = [
            tiny_skia::Point::from_xy(rect.x0 as f32, rect.y0 as f32),
            tiny_skia::Point::from_xy(rect.x1 as f32, rect.y1 as f32),
        ];
        self.base.map_points(&mut pts);
        let scale = if rect.width() > 0.0 {
            ((pts[1].x - pts[0].x) as f64 / rect.width())
                .abs()
                .max(1e-6)
        } else {
            1.0
        };
        let (w, h) = (self.width as i64, self.height as i64);
        let blur_px = (blur as f64 * scale).round().max(0.0) as i64;
        let pad = blur_px;
        let rx0 = ((pts[0].x as f64).floor() as i64 - pad).clamp(0, w);
        let ry0 = ((pts[0].y as f64).floor() as i64 - pad).clamp(0, h);
        let rx1 = ((pts[1].x as f64).ceil() as i64 + pad).clamp(0, w);
        let ry1 = ((pts[1].y as f64).ceil() as i64 + pad).clamp(0, h);
        let (rw, rh) = (rx1 - rx0, ry1 - ry0);
        if rw <= 0 || rh <= 0 {
            return;
        }

        // Snapshot the current target, crop the padded region, filter it.
        let snap = RgbaImage::from_pixmap(self.layers.last().expect("base layer"));
        let mut region = snap.crop(rx0 as u32, ry0 as u32, rw as u32, rh as u32);
        if blur_px > 0 {
            region = region.blurred(blur_px as u32);
        }
        region.saturate(saturate);

        // Composite back, clipped to the (physical) rounded rect.
        let phys_rect = Rect::new(
            pts[0].x as f64,
            pts[0].y as f64,
            pts[1].x as f64,
            pts[1].y as f64,
        );
        let phys_radii = CornerRadii {
            tl: radii.tl * scale,
            tr: radii.tr * scale,
            br: radii.br * scale,
            bl: radii.bl * scale,
        };
        let mask = path_mask(
            self.width,
            self.height,
            &rounded_rect_path(phys_rect, phys_radii),
            Transform::identity(),
        );
        let region_pm = region.to_pixmap();
        self.top().draw_pixmap(
            rx0 as i32,
            ry0 as i32,
            region_pm.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            mask.as_ref(),
        );
    }

    fn draw_rect(
        &mut self,
        rect: Rect,
        brush: &Brush,
        radii: CornerRadii,
        border: Option<&Border>,
    ) {
        let path = rounded_rect_path(rect, radii);
        self.fill_path_with(&path, brush);
        if let Some(b) = border {
            let mut paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            paint.shader = Shader::SolidColor(ts_color(b.color));
            let stroke = Stroke {
                width: b.width as f32,
                ..Default::default()
            };
            let base = self.base;
            self.top().stroke_path(&path, &paint, &stroke, base, None);
        }
    }

    fn draw_path(&mut self, path: &BezPath, brush: &Brush, style: FillOrStroke) {
        let Some(ts) = to_ts_path(path) else {
            return;
        };
        match style {
            FillOrStroke::Fill => self.fill_path_with(&ts, brush),
            FillOrStroke::Stroke { width } => {
                let mut paint = Paint {
                    anti_alias: true,
                    ..Default::default()
                };
                paint.shader = brush_shader(brush);
                let stroke = Stroke {
                    width: width as f32,
                    ..Default::default()
                };
                let base = self.base;
                self.top().stroke_path(&ts, &paint, &stroke, base, None);
            }
        }
    }

    fn fill_path_with(&mut self, path: &tiny_skia::Path, brush: &Brush) {
        if let Brush::ConicGradient {
            center,
            start_angle,
            stops,
        } = brush
        {
            self.fill_conic(path, *center, *start_angle, stops);
            return;
        }
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        paint.shader = brush_shader(brush);
        let base = self.base;
        self.top()
            .fill_path(path, &paint, FillRule::Winding, base, None);
    }

    /// Conic gradients have no tiny-skia primitive; sample per pixel into a temp
    /// pixmap, masked to the path.
    fn fill_conic(
        &mut self,
        path: &tiny_skia::Path,
        center: Point,
        start: f64,
        stops: &[GradientStop],
    ) {
        let (w, h) = (self.width, self.height);
        let (ox, oy) = self.origin;
        let Some(mask) = path_mask(w, h, path, self.base) else {
            return;
        };
        let mut tmp = Pixmap::new(w.max(1), h.max(1)).expect("valid size");
        let data = tmp.pixels_mut();
        for py in 0..h {
            for px in 0..w {
                let wx = px as f64 + ox + 0.5;
                let wy = py as f64 + oy + 0.5;
                let mut t = ((wy - center.y).atan2(wx - center.x) - start) / std::f64::consts::TAU;
                t = t.rem_euclid(1.0);
                let c = sample_stops_oklab(stops, t as f32).to_srgb8();
                let idx = (py * w + px) as usize;
                data[idx] = tiny_skia::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
            }
        }
        self.top().draw_pixmap(
            0,
            0,
            tmp.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }

    fn draw_image(&mut self, id: ImageId, src: Rect, dst: Rect, quality: Filter) {
        let Some(img) = self.images.get(id.0 as usize) else {
            return;
        };
        let sx = src.x0.max(0.0) as u32;
        let sy = src.y0.max(0.0) as u32;
        let sw = (src.width().max(0.0) as u32).min(img.width().saturating_sub(sx));
        let sh = (src.height().max(0.0) as u32).min(img.height().saturating_sub(sy));
        if sw == 0 || sh == 0 {
            return;
        }
        // Avoid copying when the whole image is used (the common case for
        // cached text/shadow sprites): crop allocates and copies every pixel.
        let src_pm = if sx == 0 && sy == 0 && sw == img.width() && sh == img.height() {
            img.to_pixmap()
        } else {
            img.crop(sx, sy, sw, sh).to_pixmap()
        };
        let local = Transform::from_row(
            (dst.width() / sw as f64) as f32,
            0.0,
            0.0,
            (dst.height() / sh as f64) as f32,
            dst.x0 as f32,
            dst.y0 as f32,
        );
        let paint = PixmapPaint {
            quality: match quality {
                Filter::Nearest => tiny_skia::FilterQuality::Nearest,
                Filter::Bilinear => tiny_skia::FilterQuality::Bilinear,
            },
            ..Default::default()
        };
        let base = self.base;
        self.top()
            .draw_pixmap(0, 0, src_pm.as_ref(), &paint, base.pre_concat(local), None);
    }

    fn push_layer(
        &mut self,
        clip: Option<RoundedRect>,
        opacity: f32,
        transform: Affine,
        blend: BlendMode,
    ) {
        self.layers
            .push(Pixmap::new(self.width.max(1), self.height.max(1)).expect("valid size"));
        self.params.push(LayerParams {
            clip,
            opacity,
            transform,
            blend,
        });
    }

    fn pop_layer(&mut self) {
        let Some(layer) = self.layers.pop() else {
            return;
        };
        let Some(p) = self.params.pop() else {
            return;
        };
        // Layer content is already in canvas space; its clip is a window-space
        // rounded rect, mapped into canvas space by `base`.
        let clip_mask = p.clip.and_then(|rr| {
            path_mask(
                self.width,
                self.height,
                &rounded_rect_path(rr.rect, rr.radii),
                self.base,
            )
        });
        let paint = PixmapPaint {
            opacity: p.opacity,
            blend_mode: ts_blend(p.blend),
            ..Default::default()
        };
        let transform = affine_to_ts(p.transform);
        self.top()
            .draw_pixmap(0, 0, layer.as_ref(), &paint, transform, clip_mask.as_ref());
    }
}

// --- conversions ------------------------------------------------------------

fn ts_color(c: Color) -> tiny_skia::Color {
    let [r, g, b, a] = c.to_srgb8();
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

fn affine_to_ts(a: Affine) -> Transform {
    let c = a.as_coeffs();
    Transform::from_row(
        c[0] as f32,
        c[1] as f32,
        c[2] as f32,
        c[3] as f32,
        c[4] as f32,
        c[5] as f32,
    )
}

fn ts_blend(b: BlendMode) -> tiny_skia::BlendMode {
    use tiny_skia::BlendMode as B;
    match b {
        BlendMode::SourceOver => B::SourceOver,
        BlendMode::Multiply => B::Multiply,
        BlendMode::Screen => B::Screen,
        BlendMode::Overlay => B::Overlay,
        BlendMode::Darken => B::Darken,
        BlendMode::Lighten => B::Lighten,
    }
}

fn ts_spread(s: SpreadMode) -> tiny_skia::SpreadMode {
    use tiny_skia::SpreadMode as S;
    match s {
        SpreadMode::Pad => S::Pad,
        SpreadMode::Repeat => S::Repeat,
        SpreadMode::Reflect => S::Reflect,
    }
}

fn to_ts_path(p: &BezPath) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(c, e) => pb.quad_to(c.x as f32, c.y as f32, e.x as f32, e.y as f32),
            PathEl::CurveTo(c1, c2, e) => pb.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                e.x as f32,
                e.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

fn rounded_rect_path(rect: Rect, radii: CornerRadii) -> tiny_skia::Path {
    if radii.is_zero() {
        let mut pb = PathBuilder::new();
        pb.push_rect(
            tiny_skia::Rect::from_xywh(
                rect.x0 as f32,
                rect.y0 as f32,
                rect.width() as f32,
                rect.height() as f32,
            )
            .expect("valid rect"),
        );
        return pb.finish().expect("rect path");
    }
    let kr = kurbo::RoundedRect::new(
        rect.x0,
        rect.y0,
        rect.x1,
        rect.y1,
        kurbo::RoundedRectRadii::new(radii.tl, radii.tr, radii.br, radii.bl),
    );
    to_ts_path(&kr.to_path(0.1)).expect("rounded rect path")
}

fn path_mask(w: u32, h: u32, path: &tiny_skia::Path, transform: Transform) -> Option<Mask> {
    let mut mask = Mask::new(w.max(1), h.max(1))?;
    mask.fill_path(path, FillRule::Winding, true, transform);
    Some(mask)
}

// --- gradients --------------------------------------------------------------

fn brush_shader(brush: &Brush) -> Shader<'static> {
    match brush {
        Brush::Solid(c) => Shader::SolidColor(ts_color(*c)),
        Brush::LinearGradient {
            start,
            end,
            stops,
            spread,
        } => LinearGradient::new(
            tiny_skia::Point::from_xy(start.x as f32, start.y as f32),
            tiny_skia::Point::from_xy(end.x as f32, end.y as f32),
            bake_stops(stops),
            ts_spread(*spread),
            Transform::identity(),
        )
        .unwrap_or(Shader::SolidColor(fallback_solid(stops))),
        Brush::RadialGradient {
            center,
            radius,
            stops,
            spread,
        } => RadialGradient::new(
            tiny_skia::Point::from_xy(center.x as f32, center.y as f32),
            tiny_skia::Point::from_xy(center.x as f32, center.y as f32),
            (*radius).max(0.001) as f32,
            bake_stops(stops),
            ts_spread(*spread),
            Transform::identity(),
        )
        .unwrap_or(Shader::SolidColor(fallback_solid(stops))),
        // Conic handled out-of-band; never reached here.
        Brush::ConicGradient { stops, .. } => Shader::SolidColor(fallback_solid(stops)),
    }
}

fn fallback_solid(stops: &[GradientStop]) -> tiny_skia::Color {
    ts_color(stops.first().map(|s| s.color).unwrap_or(Color::BLACK))
}

/// Bake user stops into many tiny-skia stops interpolated in Oklab so the
/// rasterizer's linear interpolation approximates a perceptual ramp.
fn bake_stops(stops: &[GradientStop]) -> Vec<TsStop> {
    if stops.len() < 2 {
        let c = fallback_solid(stops);
        return vec![TsStop::new(0.0, c), TsStop::new(1.0, c)];
    }
    let mut out = Vec::new();
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        for k in 0..GRADIENT_BAKE_STEPS {
            let f = k as f32 / GRADIENT_BAKE_STEPS as f32;
            let offset = a.offset + (b.offset - a.offset) * f;
            let c = a.color.lerp_oklab(b.color, f).to_srgb8();
            out.push(TsStop::new(
                offset.clamp(0.0, 1.0),
                tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3]),
            ));
        }
    }
    let last = stops[stops.len() - 1];
    let lc = last.color.to_srgb8();
    out.push(TsStop::new(
        last.offset.clamp(0.0, 1.0),
        tiny_skia::Color::from_rgba8(lc[0], lc[1], lc[2], lc[3]),
    ));
    out
}

/// Sample a stop list at `t` in `[0, 1]`, interpolating in Oklab.
fn sample_stops_oklab(stops: &[GradientStop], t: f32) -> Color {
    if stops.is_empty() {
        return Color::BLACK;
    }
    if t <= stops[0].offset {
        return stops[0].color;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.offset {
            let span = (b.offset - a.offset).max(f32::EPSILON);
            let f = (t - a.offset) / span;
            return a.color.lerp_oklab(b.color, f);
        }
    }
    stops[stops.len() - 1].color
}
