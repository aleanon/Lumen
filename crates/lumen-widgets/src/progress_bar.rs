//! [`ProgressBar`] — a determinate progress indicator. Its `Element` (track +
//! fill) is built inside [`ProgressBar::new`].

use crate::widget::{impl_widget, Common, Widget};
use crate::{BuildCx, Element};
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, LayoutStyle};

/// A horizontal bar showing `fraction` (0..=1) of a track filled.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{full_width, ProgressBar, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     full_width(cx, ProgressBar::new(0.65).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 240.0, 56.0, "progress_bar");
/// ```
///
/// Renders:
///
/// ![Progress Bar example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/progress_bar.png)
///
/// The picture above is `src/doc_shots/progress_bar.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct ProgressBar {
    /// Determinate fraction, or the resolved sweep of an indeterminate bar.
    ///
    /// The indeterminate geometry is computed in `indeterminate()`, not here:
    /// it reads the animation clock, and that read is a *tracked* dependency —
    /// deferring it to `build` would move the dependency edge.
    mode: Mode,
    width: f32,
    height: f32,
    fill_color: Option<Color>,
    common: Common,
}

/// What the bar is showing.
#[derive(Clone, Copy)]
enum Mode {
    /// A known fraction of completion, clamped to `0.0..=1.0`.
    Determinate(f64),
    /// An unknown duration: a short segment at `left`, `width` wide.
    Indeterminate { left: f32, width: f32 },
}

impl ProgressBar {
    /// A progress bar at `fraction` of completion (clamped to `0.0..=1.0`).
    pub fn new(fraction: f64) -> ProgressBar {
        ProgressBar {
            mode: Mode::Determinate(fraction.clamp(0.0, 1.0)),
            width: 200.0,
            height: 10.0,
            fill_color: None,
            common: Common::default(),
        }
    }

    /// An **indeterminate** bar: work is happening but its duration is unknown.
    ///
    /// The most common progress case, and the one Lumen had no answer for
    /// (`Spinner` is indeterminate but a different shape). Matches
    /// `LinearProgressIndicator(value: null)` / `<progress>` with no `value`:
    /// a short bar sweeps the track, and the accessible value is absent rather
    /// than a misleading percentage.
    ///
    /// Driven by the animation clock, so it is deterministic under the virtual
    /// clock in tests and goldens.
    pub fn indeterminate(cx: &BuildCx) -> ProgressBar {
        const PERIOD: f64 = 1_200.0;
        const SEGMENT: f32 = 0.3;
        cx.animate();
        let phase = ((cx.now_ms() % PERIOD) / PERIOD) as f32;
        // Sweep from fully off the left to fully off the right.
        let left = phase * (1.0 + SEGMENT) - SEGMENT;
        let (left, width) = if left < 0.0 {
            (0.0, (SEGMENT + left).max(0.0))
        } else {
            (left, SEGMENT.min(1.0 - left))
        };
        ProgressBar {
            mode: Mode::Indeterminate { left, width },
            width: 200.0,
            height: 10.0,
            fill_color: None,
            common: Common::default(),
        }
    }

    /// Set the track width in px (default 200).
    pub fn width(mut self, px: f32) -> ProgressBar {
        self.width = px;
        self
    }

    /// Set the bar height/thickness in px (default 10).
    pub fn height(mut self, px: f32) -> ProgressBar {
        self.height = px;
        self
    }

    /// Recolour the filled portion.
    pub fn fill_color(mut self, c: Color) -> ProgressBar {
        self.fill_color = Some(c);
        self
    }
}

impl Widget for ProgressBar {
    fn build(self) -> Element {
        let ProgressBar {
            mode,
            width,
            height,
            fill_color,
            common,
        } = self;
        let ink = fill_color.unwrap_or(Color::srgb8(0x1a, 0x73, 0xe8, 0xff));

        let fill_style = match mode {
            Mode::Determinate(frac) => LayoutStyle {
                width: Dim::pct(frac as f32),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            Mode::Indeterminate { left, width } => LayoutStyle {
                position: lumen_layout::Position::Absolute,
                inset: lumen_layout::Edges {
                    left: Dim::pct(left),
                    ..lumen_layout::Edges::AUTO
                },
                width: Dim::pct(width),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
        };
        let fill = Element {
            role: Role::Generic,
            elide_semantics: true,
            background: Some(ink),
            corner_radius: 5.0,
            style: fill_style,
            ..Element::default()
        }
        .part("fill");

        let mut el = Element {
            role: Role::Progress,
            value: match mode {
                Mode::Determinate(frac) => Some(format!("{:.0}%", frac * 100.0)),
                // No value: an indeterminate bar must not claim a percentage.
                Mode::Indeterminate { .. } => None,
            },
            states: match mode {
                Mode::Determinate(_) => Vec::new(),
                Mode::Indeterminate { .. } => vec![lumen_core::semantics::State::Busy],
            },
            background: Some(Color::srgb8(0xe3, 0xe6, 0xeb, 0xff)),
            corner_radius: 5.0,
            style: LayoutStyle {
                position: match mode {
                    Mode::Determinate(_) => LayoutStyle::default().position,
                    Mode::Indeterminate { .. } => lumen_layout::Position::Relative,
                },
                width: Dim::px(width),
                height: Dim::px(height),
                ..LayoutStyle::default()
            },
            children: vec![fill],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(ProgressBar);
