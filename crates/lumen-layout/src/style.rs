//! The typed layout style and its mapping onto Taffy.
//!
//! These are Lumen's own types (ADR-004: no taffy types in the public API),
//! covering the layout property set of 04 §3. `LayoutStyle::to_taffy` is the one
//! place that touches taffy.

use taffy::prelude::{auto, fr, length, percent, zero};

/// Outer display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Display {
    /// Flexbox container.
    #[default]
    Flex,
    /// CSS Grid container.
    Grid,
    /// Not displayed; contributes no layout.
    None,
}

/// Main-axis direction for flex containers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexDirection {
    /// Left to right.
    #[default]
    Row,
    /// Top to bottom.
    Column,
    /// Right to left.
    RowReverse,
    /// Bottom to top.
    ColumnReverse,
}

/// Flex wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexWrap {
    /// Single line.
    #[default]
    NoWrap,
    /// Wrap onto multiple lines.
    Wrap,
    /// Wrap, reversing cross-axis order.
    WrapReverse,
}

/// Positioning scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Position {
    /// In normal flow.
    #[default]
    Relative,
    /// Taken out of flow, positioned by `inset`.
    Absolute,
}

/// Alignment value, shared across align/justify properties (04 §3). Not every
/// value is meaningful for every property; invalid combinations map to the
/// nearest sensible Taffy value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Pack at the start.
    Start,
    /// Pack at the end.
    End,
    /// Center.
    Center,
    /// Stretch to fill.
    Stretch,
    /// Align baselines.
    Baseline,
    /// Distribute with space between items.
    SpaceBetween,
    /// Distribute with space around items.
    SpaceAround,
    /// Distribute with equal space between and around.
    SpaceEvenly,
}

/// A length: auto, logical pixels, or a fraction of the containing block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dim {
    /// Automatic sizing.
    Auto,
    /// Logical pixels.
    Px(f32),
    /// Fraction of the containing block, `0.0..=1.0`.
    Percent(f32),
}

impl Dim {
    /// Pixels constructor.
    pub fn px(v: f32) -> Dim {
        Dim::Px(v)
    }
    /// Percent constructor (fraction `0.0..=1.0`).
    pub fn pct(frac: f32) -> Dim {
        Dim::Percent(frac)
    }
}

/// A grid track size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridTrack {
    /// Fixed pixels.
    Px(f32),
    /// Fraction of the container.
    Percent(f32),
    /// Flex fraction (`fr`).
    Fr(f32),
    /// Auto-sized to content.
    Auto,
}

/// A grid line placement for one axis edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridLine {
    /// Auto placement.
    #[default]
    Auto,
    /// Span `n` tracks.
    Span(u16),
    /// A specific (1-based, may be negative) line.
    Line(i16),
}

/// Four-sided edge values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    /// Left.
    pub left: Dim,
    /// Right.
    pub right: Dim,
    /// Top.
    pub top: Dim,
    /// Bottom.
    pub bottom: Dim,
}

impl Edges {
    /// All four edges zero (the default for padding/margin).
    pub const ZERO: Edges = Edges {
        left: Dim::Px(0.0),
        right: Dim::Px(0.0),
        top: Dim::Px(0.0),
        bottom: Dim::Px(0.0),
    };
    /// All four edges `auto` (the default for `inset`).
    pub const AUTO: Edges = Edges {
        left: Dim::Auto,
        right: Dim::Auto,
        top: Dim::Auto,
        bottom: Dim::Auto,
    };
    /// The same value on every edge.
    pub fn all(d: Dim) -> Edges {
        Edges {
            left: d,
            right: d,
            top: d,
            bottom: d,
        }
    }
}

impl Default for Edges {
    fn default() -> Self {
        Edges::ZERO
    }
}

/// The typed layout style for one node (subset of 04 §3 covering M0 fixtures).
#[derive(Clone, Debug)]
pub struct LayoutStyle {
    /// Display mode.
    pub display: Display,
    /// Positioning scheme.
    pub position: Position,
    /// Flex main-axis direction.
    pub flex_direction: FlexDirection,
    /// Flex wrapping.
    pub flex_wrap: FlexWrap,
    /// Flex grow factor.
    pub flex_grow: f32,
    /// Flex shrink factor.
    pub flex_shrink: f32,
    /// Flex basis.
    pub flex_basis: Dim,
    /// Cross-axis item alignment.
    pub align_items: Option<Align>,
    /// Per-item cross-axis override.
    pub align_self: Option<Align>,
    /// Multi-line cross-axis alignment.
    pub align_content: Option<Align>,
    /// Main-axis content distribution.
    pub justify_content: Option<Align>,
    /// Gap between rows (px or percent).
    pub row_gap: Dim,
    /// Gap between columns (px or percent).
    pub column_gap: Dim,
    /// Width.
    pub width: Dim,
    /// Height.
    pub height: Dim,
    /// Minimum width.
    pub min_width: Dim,
    /// Minimum height.
    pub min_height: Dim,
    /// Maximum width.
    pub max_width: Dim,
    /// Maximum height.
    pub max_height: Dim,
    /// Aspect ratio (width / height).
    pub aspect_ratio: Option<f32>,
    /// Padding.
    pub padding: Edges,
    /// Margin.
    pub margin: Edges,
    /// Inset (for absolute positioning).
    pub inset: Edges,
    /// Grid column track list.
    pub grid_template_columns: Vec<GridTrack>,
    /// Grid row track list.
    pub grid_template_rows: Vec<GridTrack>,
    /// Grid column placement `(start, end)`.
    pub grid_column: (GridLine, GridLine),
    /// Grid row placement `(start, end)`.
    pub grid_row: (GridLine, GridLine),
}

impl Default for LayoutStyle {
    fn default() -> Self {
        LayoutStyle {
            display: Display::Flex,
            position: Position::Relative,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dim::Auto,
            align_items: None,
            align_self: None,
            align_content: None,
            justify_content: None,
            row_gap: Dim::Px(0.0),
            column_gap: Dim::Px(0.0),
            width: Dim::Auto,
            height: Dim::Auto,
            min_width: Dim::Auto,
            min_height: Dim::Auto,
            max_width: Dim::Auto,
            max_height: Dim::Auto,
            aspect_ratio: None,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            inset: Edges::AUTO,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_column: (GridLine::Auto, GridLine::Auto),
            grid_row: (GridLine::Auto, GridLine::Auto),
        }
    }
}

impl LayoutStyle {
    /// Map to a taffy `Style`. The single point of taffy coupling (ADR-004).
    pub(crate) fn to_taffy(&self) -> taffy::Style {
        use taffy::geometry::{Rect as TRect, Size as TSize};
        let mut s = taffy::Style {
            display: match self.display {
                Display::Flex => taffy::Display::Flex,
                Display::Grid => taffy::Display::Grid,
                Display::None => taffy::Display::None,
            },
            position: match self.position {
                Position::Relative => taffy::Position::Relative,
                Position::Absolute => taffy::Position::Absolute,
            },
            flex_direction: match self.flex_direction {
                FlexDirection::Row => taffy::FlexDirection::Row,
                FlexDirection::Column => taffy::FlexDirection::Column,
                FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
                FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
            },
            flex_wrap: match self.flex_wrap {
                FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
                FlexWrap::Wrap => taffy::FlexWrap::Wrap,
                FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
            },
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: dim(self.flex_basis),
            align_items: self.align_items.map(align_items),
            align_self: self.align_self.map(align_items),
            align_content: self.align_content.map(align_content),
            justify_content: self.justify_content.map(align_content),
            gap: TSize {
                width: lp(self.column_gap),
                height: lp(self.row_gap),
            },
            size: TSize {
                width: dim(self.width),
                height: dim(self.height),
            },
            min_size: TSize {
                width: dim(self.min_width),
                height: dim(self.min_height),
            },
            max_size: TSize {
                width: dim(self.max_width),
                height: dim(self.max_height),
            },
            aspect_ratio: self.aspect_ratio,
            padding: TRect {
                left: lp(self.padding.left),
                right: lp(self.padding.right),
                top: lp(self.padding.top),
                bottom: lp(self.padding.bottom),
            },
            margin: TRect {
                left: lpa(self.margin.left),
                right: lpa(self.margin.right),
                top: lpa(self.margin.top),
                bottom: lpa(self.margin.bottom),
            },
            inset: TRect {
                left: lpa(self.inset.left),
                right: lpa(self.inset.right),
                top: lpa(self.inset.top),
                bottom: lpa(self.inset.bottom),
            },
            ..Default::default()
        };
        if self.display == Display::Grid {
            s.grid_template_columns = self
                .grid_template_columns
                .iter()
                .copied()
                .map(track)
                .collect();
            s.grid_template_rows = self.grid_template_rows.iter().copied().map(track).collect();
            s.grid_column = taffy::geometry::Line {
                start: placement(self.grid_column.0),
                end: placement(self.grid_column.1),
            };
            s.grid_row = taffy::geometry::Line {
                start: placement(self.grid_row.0),
                end: placement(self.grid_row.1),
            };
        }
        s
    }
}

fn dim(d: Dim) -> taffy::Dimension {
    match d {
        Dim::Auto => auto(),
        Dim::Px(v) => length(v),
        Dim::Percent(f) => percent(f),
    }
}

/// Length-or-percent (no `auto`): used for padding and gap, where `auto`
/// is not meaningful and maps to zero.
fn lp(d: Dim) -> taffy::LengthPercentage {
    match d {
        Dim::Auto => zero(),
        Dim::Px(v) => length(v),
        Dim::Percent(f) => percent(f),
    }
}

fn lpa(d: Dim) -> taffy::LengthPercentageAuto {
    match d {
        Dim::Auto => auto(),
        Dim::Px(v) => length(v),
        Dim::Percent(f) => percent(f),
    }
}

fn track(t: GridTrack) -> taffy::TrackSizingFunction {
    match t {
        GridTrack::Px(v) => length(v),
        GridTrack::Percent(f) => percent(f),
        GridTrack::Fr(v) => fr(v),
        GridTrack::Auto => auto(),
    }
}

fn placement(l: GridLine) -> taffy::GridPlacement {
    match l {
        GridLine::Auto => taffy::GridPlacement::Auto,
        GridLine::Span(n) => taffy::GridPlacement::Span(n),
        GridLine::Line(i) => taffy::prelude::line(i),
    }
}

fn align_items(a: Align) -> taffy::AlignItems {
    match a {
        Align::Start => taffy::AlignItems::Start,
        Align::End => taffy::AlignItems::End,
        Align::Center => taffy::AlignItems::Center,
        Align::Stretch => taffy::AlignItems::Stretch,
        Align::Baseline => taffy::AlignItems::Baseline,
        // not meaningful for items; fall back to stretch
        Align::SpaceBetween | Align::SpaceAround | Align::SpaceEvenly => taffy::AlignItems::Stretch,
    }
}

fn align_content(a: Align) -> taffy::AlignContent {
    match a {
        Align::Start => taffy::AlignContent::Start,
        Align::End => taffy::AlignContent::End,
        Align::Center => taffy::AlignContent::Center,
        Align::Stretch => taffy::AlignContent::Stretch,
        Align::Baseline => taffy::AlignContent::Start,
        Align::SpaceBetween => taffy::AlignContent::SpaceBetween,
        Align::SpaceAround => taffy::AlignContent::SpaceAround,
        Align::SpaceEvenly => taffy::AlignContent::SpaceEvenly,
    }
}
