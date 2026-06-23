//! [`ProgressBar`] — a determinate progress indicator. Its `Element` (track +
//! fill) is built inside [`ProgressBar::new`].

use crate::widget::impl_common;
use crate::Element;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_layout::{Dim, LayoutStyle};

/// A horizontal bar showing `fraction` (0..=1) of a track filled.
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
        };
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
