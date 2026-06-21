//! chart — a vertical slice proving the `Frame` text API + constraint-aware leaf
//! measure are enough to build real charts. `LineChart` and `BarChart` are
//! [`LeafWidget`]s: they `measure` (so they flex to the panel), `paint` axes /
//! gridlines / series with `Frame` geometry, draw tick + value labels with
//! `Frame::fill_text`, and expose `semantics` so they're accessible + testable.
use kurbo::{BezPath, Point, Rect, Size};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_render::canvas::{AnchorX, AnchorY, Frame, TextOpts};

use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element, LeafWidget};

use lumen_layout::{Align, Dim, Edges};

/// Build the chart app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

// ---- palette (chart geometry colours live in code, not `.lss`; `srgb8` does an
// sRGB→linear conversion so it can't be a `const`) ----
fn grid() -> Color {
    Color::srgb8(0x24, 0x2f, 0x49, 0xff)
}
fn axis_text() -> Color {
    Color::srgb8(0x8b, 0x95, 0xad, 0xff)
}
fn line_col() -> Color {
    Color::srgb8(0x5b, 0x9c, 0xff, 0xff)
}
fn area_col() -> Color {
    Color::srgb8(0x5b, 0x9c, 0xff, 0x33)
}
fn dot_col() -> Color {
    Color::srgb8(0xbf, 0xd6, 0xff, 0xff)
}
fn bar_top() -> Color {
    Color::srgb8(0x2d, 0xd4, 0xbf, 0xff)
}
fn bar_bot() -> Color {
    Color::srgb8(0x14, 0x9c, 0xa8, 0xff)
}

/// "Nice" upper bound for an axis: round `max` up to 1/2/5 × 10ⁿ so ticks land
/// on readable numbers.
fn nice_max(max: f64) -> f64 {
    if max <= 0.0 {
        return 1.0;
    }
    let pow = 10f64.powf(max.log10().floor());
    for step in [1.0, 2.0, 2.5, 5.0, 10.0] {
        let cand = step * pow;
        if cand >= max {
            return cand;
        }
    }
    10.0 * pow
}

fn tick_label(v: f64) -> String {
    if v.fract().abs() < 1e-6 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// A line chart with a filled area, gridlines, dots, and axis labels.
pub struct LineChart {
    /// Y values, evenly spaced along X.
    pub values: Vec<f64>,
    /// One label per value, drawn under the X axis.
    pub x_labels: Vec<String>,
}

impl LeafWidget for LineChart {
    fn measure(&self, available: Size) -> Size {
        Size::new(
            if available.width.is_finite() {
                available.width
            } else {
                360.0
            },
            if available.height.is_finite() {
                available.height
            } else {
                200.0
            },
        )
    }

    fn paint(&self, f: &mut Frame, size: Size) {
        let (ml, mr, mt, mb) = (40.0, 12.0, 12.0, 26.0);
        let plot = Rect::new(ml, mt, size.width - mr, size.height - mb);
        let max = nice_max(self.values.iter().cloned().fold(0.0, f64::max));
        let n = self.values.len();

        // Horizontal gridlines + Y tick labels at 0 / ½ / 1 of the range.
        for k in 0..=2 {
            let frac = k as f64 / 2.0;
            let y = plot.y1 - frac * plot.height();
            let mut g = BezPath::new();
            g.move_to((plot.x0, y));
            g.line_to((plot.x1, y));
            f.stroke(&g, grid(), 1.0);
            f.fill_text(
                Point::new(plot.x0 - 8.0, y),
                tick_label(frac * max),
                TextOpts {
                    size: 11.0,
                    color: axis_text(),
                    anchor_x: AnchorX::End,
                    anchor_y: AnchorY::Middle,
                    ..TextOpts::default()
                },
            );
        }

        if n < 2 {
            return;
        }
        let x_at = |i: usize| plot.x0 + (i as f64 / (n - 1) as f64) * plot.width();
        let y_at = |v: f64| plot.y1 - (v / max) * plot.height();
        let pts: Vec<Point> = self
            .values
            .iter()
            .enumerate()
            .map(|(i, &v)| Point::new(x_at(i), y_at(v)))
            .collect();

        // Filled area under the curve.
        let mut area = BezPath::new();
        area.move_to((pts[0].x, plot.y1));
        for p in &pts {
            area.line_to(*p);
        }
        area.line_to((pts[n - 1].x, plot.y1));
        area.close_path();
        f.fill(&area, area_col());

        // The line itself.
        let mut line = BezPath::new();
        line.move_to(pts[0]);
        for p in &pts[1..] {
            line.line_to(*p);
        }
        f.stroke(&line, line_col(), 2.5);

        // Dots + X labels.
        for (i, p) in pts.iter().enumerate() {
            f.fill_circle(*p, 3.0, dot_col());
            if let Some(lbl) = self.x_labels.get(i) {
                f.fill_text(
                    Point::new(p.x, plot.y1 + 7.0),
                    lbl,
                    TextOpts {
                        size: 11.0,
                        color: axis_text(),
                        anchor_x: AnchorX::Center,
                        anchor_y: AnchorY::Top,
                        ..TextOpts::default()
                    },
                );
            }
        }
    }

    fn semantics(&self) -> (Role, String) {
        (
            Role::Image,
            format!("Line chart, {} points", self.values.len()),
        )
    }
}

/// A vertical bar chart with value labels above each bar.
pub struct BarChart {
    /// (category label, value) pairs.
    pub data: Vec<(String, f64)>,
}

impl LeafWidget for BarChart {
    fn measure(&self, available: Size) -> Size {
        Size::new(
            if available.width.is_finite() {
                available.width
            } else {
                360.0
            },
            if available.height.is_finite() {
                available.height
            } else {
                200.0
            },
        )
    }

    fn paint(&self, f: &mut Frame, size: Size) {
        let (ml, mr, mt, mb) = (40.0, 12.0, 18.0, 26.0);
        let plot = Rect::new(ml, mt, size.width - mr, size.height - mb);
        let max = nice_max(self.data.iter().map(|(_, v)| *v).fold(0.0, f64::max));

        for k in 0..=2 {
            let frac = k as f64 / 2.0;
            let y = plot.y1 - frac * plot.height();
            let mut g = BezPath::new();
            g.move_to((plot.x0, y));
            g.line_to((plot.x1, y));
            f.stroke(&g, grid(), 1.0);
            f.fill_text(
                Point::new(plot.x0 - 8.0, y),
                tick_label(frac * max),
                TextOpts {
                    size: 11.0,
                    color: axis_text(),
                    anchor_x: AnchorX::End,
                    anchor_y: AnchorY::Middle,
                    ..TextOpts::default()
                },
            );
        }

        let n = self.data.len();
        if n == 0 {
            return;
        }
        let slot = plot.width() / n as f64;
        let bw = slot * 0.56;
        for (i, (label, v)) in self.data.iter().enumerate() {
            let cx = plot.x0 + (i as f64 + 0.5) * slot;
            let h = (v / max) * plot.height();
            let bar = Rect::new(cx - bw / 2.0, plot.y1 - h, cx + bw / 2.0, plot.y1);
            // A two-stop (horizontal) gradient gives the bar a bit of sheen.
            f.linear_gradient_rect(bar, bar_top(), bar_bot());
            f.fill_text(
                Point::new(cx, plot.y1 - h - 5.0),
                tick_label(*v),
                TextOpts {
                    size: 11.0,
                    weight: 700.0,
                    color: bar_top(),
                    anchor_x: AnchorX::Center,
                    anchor_y: AnchorY::Bottom,
                },
            );
            f.fill_text(
                Point::new(cx, plot.y1 + 7.0),
                label,
                TextOpts {
                    size: 11.0,
                    color: axis_text(),
                    anchor_x: AnchorX::Center,
                    anchor_y: AnchorY::Top,
                    ..TextOpts::default()
                },
            );
        }
    }

    fn semantics(&self) -> (Role, String) {
        (Role::Image, format!("Bar chart, {} bars", self.data.len()))
    }
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// Wrap a chart leaf in a titled panel that stretches to the card width — the
/// leaf flexes with it because an explicit width now survives leaf measure.
fn panel(title: &str, chart: Element, id: &str) -> Element {
    let mut chart = chart;
    chart.style.width = Dim::pct(1.0);
    chart.style.height = Dim::px(200.0);

    let mut c = widgets::column(vec![txt(title, 13.0, 700.0).class("panel-title"), chart])
        .class("panel")
        .id(id);
    c.style.row_gap = Dim::px(10.0);
    c.style.padding = Edges::all(Dim::px(16.0));
    c.style.width = Dim::pct(1.0);
    c
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;

    let line = widgets::leaf(LineChart {
        values: vec![12.0, 18.0, 9.0, 22.0, 28.0, 19.0, 34.0],
        x_labels: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    })
    .id("line-chart");
    let bars = widgets::leaf(BarChart {
        data: [("Q1", 48.0), ("Q2", 63.0), ("Q3", 39.0), ("Q4", 71.0)]
            .iter()
            .map(|(l, v)| (l.to_string(), *v))
            .collect(),
    })
    .id("bar-chart");

    let mut card = widgets::column(vec![
        txt("Analytics", 24.0, 800.0).class("title"),
        txt("LeafWidget charts drawn with the Frame API.", 14.0, 400.0).class("subtitle"),
        panel("Active users · this week", line, "line"),
        panel("Revenue · by quarter", bars, "bars"),
    ])
    .id("card");
    card.style.align_items = Some(Align::Stretch);
    card.style.row_gap = Dim::px(16.0);
    card.style.padding = Edges::all(Dim::px(28.0));
    card.style.width = Dim::px(540.0);
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
