//! Which `LayoutStyle` fields do real nodes actually set?
//!
//! `LayoutStyle` is 256 of the 339 bytes per node — the dominant column, and
//! the third uniform record in a row (Element 1072, Meta 656, LayoutStyle 256).
//! Before splitting it, measure: a field almost nobody sets belongs behind a
//! pointer, and a field almost everybody sets must stay inline. Guessing that
//! wrong is how step 2's plan went astray.

use lumen_layout::LayoutStyle;
use lumen_widgets::direct::TreeSink;
use lumen_widgets::{Button, Card, Chip, Container, Element, Label, ProgressBar};

/// A realistic screen: the widget set an app actually composes.
fn app_tree() -> Vec<Element> {
    let mut out = Vec::new();
    for i in 0..200 {
        out.push(
            Container::new(vec![
                Label::new(format!("row {i}")).size(14.0).into(),
                ProgressBar::new(i as f64 / 200.0).into(),
                Button::new("Open").ghost().on_press(|_| {}).into(),
                Chip::new("tag").into(),
            ])
            .row()
            .gap(8.0)
            .padding(4.0)
            .into(),
        );
        out.push(Card::new(vec![Label::new("card body").into()]).into());
    }
    out
}

fn main() {
    let mut sink = TreeSink::new();
    let root = lumen_widgets::widgets::column(app_tree());
    lumen_widgets::direct::lower_element(root, &mut sink, None);

    let d = LayoutStyle::default();
    let styles: Vec<LayoutStyle> = sink
        .tree
        .iter_live()
        .filter(|n| sink.meta.contains(*n))
        .map(|n| sink.meta.layout_style(n))
        .collect();
    let n = styles.len();
    let pct = |c: usize| c as f64 / n as f64 * 100.0;
    let count = |f: &dyn Fn(&LayoutStyle) -> bool| styles.iter().filter(|s| f(s)).count();

    println!("\nLayoutStyle field occupancy over {n} real nodes");
    println!("──────────────────────────────────────────────────────────────");
    let mut rows: Vec<(&str, usize, usize)> = vec![
        ("width", count(&|s| s.width != d.width), 8),
        ("height", count(&|s| s.height != d.height), 8),
        ("padding", count(&|s| s.padding != d.padding), 32),
        ("display", count(&|s| s.display != d.display), 1),
        (
            "flex_direction",
            count(&|s| s.flex_direction != d.flex_direction),
            1,
        ),
        ("flex_grow", count(&|s| s.flex_grow != d.flex_grow), 4),
        ("flex_shrink", count(&|s| s.flex_shrink != d.flex_shrink), 4),
        ("flex_basis", count(&|s| s.flex_basis != d.flex_basis), 8),
        ("align_items", count(&|s| s.align_items != d.align_items), 2),
        ("align_self", count(&|s| s.align_self != d.align_self), 2),
        (
            "align_content",
            count(&|s| s.align_content != d.align_content),
            2,
        ),
        (
            "justify_content",
            count(&|s| s.justify_content != d.justify_content),
            2,
        ),
        ("row_gap", count(&|s| s.row_gap != d.row_gap), 8),
        ("column_gap", count(&|s| s.column_gap != d.column_gap), 8),
        ("position", count(&|s| s.position != d.position), 1),
        ("margin", count(&|s| s.margin != d.margin), 32),
        ("inset", count(&|s| s.inset != d.inset), 32),
        ("min_width", count(&|s| s.min_width != d.min_width), 8),
        ("min_height", count(&|s| s.min_height != d.min_height), 8),
        ("max_width", count(&|s| s.max_width != d.max_width), 8),
        ("max_height", count(&|s| s.max_height != d.max_height), 8),
        (
            "aspect_ratio",
            count(&|s| s.aspect_ratio != d.aspect_ratio),
            8,
        ),
        ("flex_wrap", count(&|s| s.flex_wrap != d.flex_wrap), 1),
        (
            "grid_template_columns",
            count(&|s| !s.grid_template_columns.is_empty()),
            24,
        ),
        (
            "grid_template_rows",
            count(&|s| !s.grid_template_rows.is_empty()),
            24,
        ),
        ("grid_column", count(&|s| s.grid_column != d.grid_column), 8),
        ("grid_row", count(&|s| s.grid_row != d.grid_row), 8),
    ];
    rows.sort_by_key(|(_, c, _)| std::cmp::Reverse(*c));
    println!(
        "  {:<24}{:>8}{:>9}{:>8}",
        "field", "set", "of nodes", "bytes"
    );
    for (name, c, b) in &rows {
        println!("  {:<24}{:>8}{:>8.1}%{:>8}", name, c, pct(*c), b);
    }

    let cold: usize = rows
        .iter()
        .filter(|(_, c, _)| pct(*c) < 5.0)
        .map(|(_, _, b)| b)
        .sum();
    let hot: usize = rows
        .iter()
        .filter(|(_, c, _)| pct(*c) >= 5.0)
        .map(|(_, _, b)| b)
        .sum();
    println!("──────────────────────────────────────────────────────────────");
    println!(
        "  size_of::<LayoutStyle>()   : {:>5} B",
        std::mem::size_of::<LayoutStyle>()
    );
    println!("  fields set by >=5% of nodes: {hot:>5} B");
    println!("  fields set by  <5% of nodes: {cold:>5} B   <- the split");
    println!();
}
