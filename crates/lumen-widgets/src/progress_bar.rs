//! [`ProgressBar`] — a determinate progress indicator. Its `Element` (track +
//! fill) is built inside [`ProgressBar::new`].

use crate::widget::impl_common;
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
    el: Element,
}

impl ProgressBar {
    /// A progress bar at `fraction` of completion (clamped to `0.0..=1.0`).
    pub fn new(fraction: f64) -> ProgressBar {
        let frac = fraction.clamp(0.0, 1.0);
        let fill = Element {
            role: Role::Generic,
            elide_semantics: true,
            background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
            corner_radius: 5.0,
            style: LayoutStyle {
                width: Dim::pct(frac as f32),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .part("fill");
        let el = Element {
            role: Role::Progress,
            value: Some(format!("{:.0}%", frac * 100.0)),
            background: Some(Color::srgb8(0xe3, 0xe6, 0xeb, 0xff)),
            corner_radius: 5.0,
            style: LayoutStyle {
                width: Dim::px(200.0),
                height: Dim::px(10.0),
                ..LayoutStyle::default()
            },
            children: vec![fill],
            ..Element::default()
        };
        ProgressBar { el }
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
        let fill = Element {
            role: Role::Generic,
            elide_semantics: true,
            background: Some(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
            corner_radius: 5.0,
            style: LayoutStyle {
                position: lumen_layout::Position::Absolute,
                inset: lumen_layout::Edges {
                    left: Dim::pct(left),
                    ..lumen_layout::Edges::AUTO
                },
                width: Dim::pct(width),
                height: Dim::pct(1.0),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .part("fill");
        let el = Element {
            role: Role::Progress,
            // No value: an indeterminate bar must not claim a percentage.
            states: vec![lumen_core::semantics::State::Busy],
            background: Some(Color::srgb8(0xe3, 0xe6, 0xeb, 0xff)),
            corner_radius: 5.0,
            style: LayoutStyle {
                position: lumen_layout::Position::Relative,
                width: Dim::px(200.0),
                height: Dim::px(10.0),
                ..LayoutStyle::default()
            },
            children: vec![fill],
            ..Element::default()
        };
        ProgressBar { el }
    }

    /// Set the track width in px (default 200).
    pub fn width(mut self, px: f32) -> ProgressBar {
        self.el.style.width = Dim::px(px);
        self
    }

    /// Set the bar height/thickness in px (default 10).
    pub fn height(mut self, px: f32) -> ProgressBar {
        self.el.style.height = Dim::px(px);
        self
    }

    /// Recolour the filled portion.
    pub fn fill_color(mut self, c: Color) -> ProgressBar {
        if let Some(fill) = self.el.children.first_mut() {
            fill.background = Some(c);
        }
        self
    }
}

impl_common!(ProgressBar);
