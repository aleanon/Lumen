//! Step 4 — `LayoutStyle` split by measured occupancy.
//!
//! The dominant column at 256 of 339 bytes per node, and the third uniform
//! record in a row: Element 1072, Meta 656, LayoutStyle 256. Occupancy over
//! 1801 real nodes said which half is which — `padding` at 44%, width/height
//! and the gaps at 22%, and **twenty fields set by 0.0% of nodes**, including
//! every grid field, `margin`, `inset` and all four min/max dimensions.
//!
//! The risk in a split like this is not speed, it is a field quietly lost on
//! the way through — a layout bug that shows up as "the box is the wrong size"
//! three screens away from the cause. So the round trip is exhaustive rather
//! than illustrative: every field, set to a non-default value, at once.

use lumen_layout::{
    Align, Dim, Display, Edges, FlexDirection, FlexWrap, GridLine, GridTrack, LayoutStyle, Position,
};
use lumen_widgets::direct::CompactStyle;

/// A style with **every** field moved off its default.
fn fully_populated() -> LayoutStyle {
    LayoutStyle {
        display: Display::Grid,
        position: Position::Absolute,
        flex_direction: FlexDirection::RowReverse,
        flex_wrap: FlexWrap::Wrap,
        flex_grow: 2.5,
        flex_shrink: 0.25,
        flex_basis: Dim::px(11.0),
        align_items: Some(Align::End),
        align_self: Some(Align::Center),
        align_content: Some(Align::Start),
        justify_content: Some(Align::SpaceBetween),
        row_gap: Dim::px(3.0),
        column_gap: Dim::px(4.0),
        width: Dim::px(101.0),
        height: Dim::pct(0.5),
        min_width: Dim::px(7.0),
        min_height: Dim::px(8.0),
        max_width: Dim::px(900.0),
        max_height: Dim::px(901.0),
        aspect_ratio: Some(1.75),
        padding: Edges {
            left: Dim::px(1.0),
            right: Dim::px(2.0),
            top: Dim::px(3.0),
            bottom: Dim::px(4.0),
        },
        margin: Edges {
            left: Dim::px(5.0),
            right: Dim::px(6.0),
            top: Dim::px(7.0),
            bottom: Dim::px(8.0),
        },
        inset: Edges {
            left: Dim::px(9.0),
            right: Dim::px(10.0),
            top: Dim::px(11.0),
            bottom: Dim::px(12.0),
        },
        grid_template_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(2.0)],
        grid_template_rows: vec![GridTrack::Fr(3.0)],
        grid_column: (GridLine::Line(1), GridLine::Line(3)),
        grid_row: (GridLine::Line(2), GridLine::Line(4)),
    }
}

/// Compare field by field, so a failure names the field that was lost.
fn assert_same(a: &LayoutStyle, b: &LayoutStyle) {
    macro_rules! same {
        ($($f:ident),* $(,)?) => {$(
            assert_eq!(a.$f, b.$f, concat!("field `", stringify!($f), "` did not survive the split"));
        )*};
    }
    same!(
        display,
        position,
        flex_direction,
        flex_wrap,
        flex_grow,
        flex_shrink,
        flex_basis,
        align_items,
        align_self,
        align_content,
        justify_content,
        row_gap,
        column_gap,
        width,
        height,
        min_width,
        min_height,
        max_width,
        max_height,
        aspect_ratio,
        padding,
        margin,
        inset,
        grid_template_columns,
        grid_template_rows,
        grid_column,
        grid_row,
    );
}

#[test]
fn every_field_survives_the_round_trip() {
    let original = fully_populated();
    let round_tripped = CompactStyle::from_layout(&original).to_layout();
    assert_same(&original, &round_tripped);
}

#[test]
fn a_default_style_round_trips_and_allocates_nothing() {
    let d = LayoutStyle::default();
    let c = CompactStyle::from_layout(&d);
    assert!(
        c.rare.is_none(),
        "a default style must not allocate its rare half — that is the entire \
         point of the split"
    );
    assert_same(&d, &c.to_layout());
}

#[test]
fn a_typical_node_does_not_allocate_the_rare_half() {
    // The shape the occupancy probe found: padding, size, gaps, direction.
    let typical = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        width: Dim::px(320.0),
        height: Dim::px(24.0),
        row_gap: Dim::px(8.0),
        column_gap: Dim::px(8.0),
        align_items: Some(Align::Center),
        padding: Edges::all(Dim::px(4.0)),
        ..LayoutStyle::default()
    };
    let c = CompactStyle::from_layout(&typical);
    assert!(
        c.rare.is_none(),
        "the 44%/22%/11% fields all live inline, so a normal flow node pays no \
         pointer chase and no allocation"
    );
    assert_same(&typical, &c.to_layout());
}

#[test]
fn each_rare_field_alone_triggers_the_allocation() {
    // If any one of them were forgotten in the `needs_rare` test, that field
    // would be silently dropped for a node that set only it.
    let d = LayoutStyle::default();
    let cases: Vec<(&str, LayoutStyle)> = vec![
        (
            "margin",
            LayoutStyle {
                margin: Edges::all(Dim::px(1.0)),
                ..d.clone()
            },
        ),
        (
            "inset",
            LayoutStyle {
                inset: Edges::all(Dim::px(1.0)),
                ..d.clone()
            },
        ),
        (
            "min_width",
            LayoutStyle {
                min_width: Dim::px(1.0),
                ..d.clone()
            },
        ),
        (
            "min_height",
            LayoutStyle {
                min_height: Dim::px(1.0),
                ..d.clone()
            },
        ),
        (
            "max_width",
            LayoutStyle {
                max_width: Dim::px(1.0),
                ..d.clone()
            },
        ),
        (
            "max_height",
            LayoutStyle {
                max_height: Dim::px(1.0),
                ..d.clone()
            },
        ),
        (
            "aspect_ratio",
            LayoutStyle {
                aspect_ratio: Some(2.0),
                ..d.clone()
            },
        ),
        (
            "grid_template_columns",
            LayoutStyle {
                grid_template_columns: vec![GridTrack::Fr(1.0)],
                ..d.clone()
            },
        ),
        (
            "grid_template_rows",
            LayoutStyle {
                grid_template_rows: vec![GridTrack::Fr(1.0)],
                ..d.clone()
            },
        ),
        (
            "grid_column",
            LayoutStyle {
                grid_column: (GridLine::Line(1), GridLine::Line(2)),
                ..d.clone()
            },
        ),
        (
            "grid_row",
            LayoutStyle {
                grid_row: (GridLine::Line(1), GridLine::Line(2)),
                ..d.clone()
            },
        ),
    ];
    for (name, style) in cases {
        let c = CompactStyle::from_layout(&style);
        assert!(
            c.rare.is_some(),
            "setting `{name}` alone must allocate the rare half, or it is lost"
        );
        assert_same(&style, &c.to_layout());
    }
}

#[test]
fn the_split_actually_shrinks_the_column() {
    use std::mem::size_of;
    let full = size_of::<LayoutStyle>();
    let compact = size_of::<CompactStyle>();
    assert!(
        compact < full,
        "CompactStyle ({compact} B) must be smaller than LayoutStyle ({full} B)"
    );
    // The measured occupancy said the cold half is 193 of 256 bytes, so the
    // reduction should be substantial rather than incidental.
    assert!(
        compact * 2 <= full,
        "expected at least a 2x reduction; got {full} -> {compact}"
    );
}
