//! [`LineChart`] and [`PieChart`] (W.2) — canvas-drawn charts as
//! [`LeafWidget`]s (LineChart promoted from `examples/chart`; the example
//! keeps its richer themed copy). Both measure to the available box, paint
//! with `Frame` geometry + text, and expose value-bearing semantics.

use crate::{widgets, Element, LeafWidget};
use kurbo::{BezPath, Point, Rect, Size};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_render::canvas::{AnchorX, AnchorY, Frame, TextOpts};

fn grid() -> Color {
    Color::srgb8(0xe3, 0xe6, 0xeb, 0xff)
}
fn axis_text() -> Color {
    Color::srgb8(0x6b, 0x72, 0x80, 0xff)
}

fn nice_max(raw: f64) -> f64 {
    if raw <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(raw.log10().floor());
    (raw / mag).ceil() * mag
}

fn tick_label(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.1}k", v / 1000.0)
    } else if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// A line chart leaf: evenly-spaced Y values, optional X labels, filled area.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, LineChart, BuildCx, Element};
/// use lumen_layout::Dim;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let mut chart = LineChart::element(
///         vec![3.0, 7.0, 5.0, 9.0, 6.0],
///         vec!["Mon".into(), "Tue".into(), "Wed".into(), "Thu".into(), "Fri".into()],
///     );
///     // The chart leaf fills its box — size it so the plot isn't clipped.
///     chart.style.width = Dim::px(220.0);
///     chart.style.height = Dim::px(120.0);
///     centered(cx, chart)
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 260.0, 160.0, "line_chart");
/// ```
///
/// Renders:
///
/// ![Line Chart example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/line_chart.png)
///
/// The picture above is `src/doc_shots/line_chart.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct LineChart {
    /// Y values, evenly spaced along X.
    pub values: Vec<f64>,
    /// One label per value, drawn under the X axis (may be shorter).
    pub x_labels: Vec<String>,
    /// Line color.
    pub color: Color,
}

impl LineChart {
    /// Mount as an element (leaf measure flexes to the panel).
    pub fn element(values: Vec<f64>, x_labels: Vec<String>) -> Element {
        widgets::leaf(LineChart {
            values,
            x_labels,
            color: crate::theme::accent(),
        })
        .class("line-chart")
    }
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

        let mut area = BezPath::new();
        area.move_to((pts[0].x, plot.y1));
        for p in &pts {
            area.line_to(*p);
        }
        area.line_to((pts[n - 1].x, plot.y1));
        area.close_path();
        f.fill(
            &area,
            lumen_core::Color {
                a: 0.18,
                ..self.color
            },
        );

        let mut line = BezPath::new();
        line.move_to(pts[0]);
        for p in &pts[1..] {
            line.line_to(*p);
        }
        f.stroke(&line, self.color, 2.5);

        for (i, p) in pts.iter().enumerate() {
            f.fill_circle(*p, 3.0, self.color);
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

/// One pie slice.
#[derive(Clone, Debug)]
pub struct PieSlice {
    /// Legend label.
    pub label: String,
    /// Slice weight (any non-negative scale).
    pub value: f64,
    /// Slice color.
    pub color: Color,
}

/// A pie chart leaf.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, PieChart, PieSlice, BuildCx, Element};
/// use lumen_core::Color;
/// use lumen_layout::Dim;
///
/// fn build(cx: &mut BuildCx) -> Element {
///     let mut chart = PieChart::element(vec![
///         PieSlice { label: "A".into(), value: 3.0, color: Color::srgb8(0x1a,0x73,0xe8,0xff) },
///         PieSlice { label: "B".into(), value: 2.0, color: Color::srgb8(0x2e,0xa0,0x43,0xff) },
///         PieSlice { label: "C".into(), value: 1.0, color: Color::srgb8(0xe8,0x40,0x4b,0xff) },
///     ]);
///     // The chart leaf fills its box, so give it an explicit square and center
///     // it — a larger frame keeps the whole circle visible with a margin.
///     chart.style.width = Dim::px(150.0);
///     chart.style.height = Dim::px(150.0);
///     centered(cx, chart)
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 200.0, "pie_chart");
/// ```
///
/// Renders:
///
/// ![Pie Chart example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/pie_chart.png)
///
/// The picture above is `src/doc_shots/pie_chart.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct PieChart {
    /// The slices, drawn clockwise from 12 o'clock.
    pub slices: Vec<PieSlice>,
}

impl PieChart {
    /// Mount as an element.
    pub fn element(slices: Vec<PieSlice>) -> Element {
        widgets::leaf(PieChart { slices }).class("pie-chart")
    }
}

impl LeafWidget for PieChart {
    fn measure(&self, available: Size) -> Size {
        let d = available.width.min(available.height).clamp(80.0, 220.0);
        Size::new(
            if d.is_finite() { d } else { 160.0 },
            if d.is_finite() { d } else { 160.0 },
        )
    }

    fn paint(&self, f: &mut Frame, size: Size) {
        let total: f64 = self.slices.iter().map(|s| s.value.max(0.0)).sum();
        if total <= 0.0 {
            return;
        }
        let c = Point::new(size.width / 2.0, size.height / 2.0);
        let r = size.width.min(size.height) / 2.0 - 2.0;
        let mut start = -std::f64::consts::FRAC_PI_2; // 12 o'clock
        for s in &self.slices {
            let sweep = (s.value.max(0.0) / total) * std::f64::consts::TAU;
            // Slice as a closed wedge (center → arc → center), the arc
            // flattened into short segments.
            let mut p = BezPath::new();
            p.move_to(c);
            let steps = ((sweep / 0.15).ceil() as usize).max(2);
            for k in 0..=steps {
                let a = start + sweep * (k as f64 / steps as f64);
                p.line_to(Point::new(c.x + r * a.cos(), c.y + r * a.sin()));
            }
            p.close_path();
            f.fill(&p, s.color);
            start += sweep;
        }
    }

    fn semantics(&self) -> (Role, String) {
        let total: f64 = self.slices.iter().map(|s| s.value.max(0.0)).sum();
        let parts: Vec<String> = self
            .slices
            .iter()
            .map(|s| {
                let pct = if total > 0.0 {
                    (s.value.max(0.0) / total * 100.0).round()
                } else {
                    0.0
                };
                format!("{} {pct}%", s.label)
            })
            .collect();
        (Role::Image, format!("Pie chart: {}", parts.join(", ")))
    }
}
