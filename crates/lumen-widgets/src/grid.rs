//! [`Grid`] — a virtualized 2D scrolling/resizing surface you fill with anything.
//!
//! The grid owns the hard, reusable mechanics of a spreadsheet-style surface and
//! knows nothing about what a cell *contains*:
//!
//! * **Virtualization** — only the cells in the viewport are materialized, over a
//!   `u32 × u32` index space (~4 billion each way), so it stays O(viewport) no
//!   matter how far you scroll.
//! * **2D scroll** with the wheel (Shift + wheel scrolls horizontally), draggable
//!   **scrollbars**, and optional **Ctrl/Cmd + wheel zoom** (scales the geometry).
//! * **Frozen headers** (a top column-header row and/or a left row-header column)
//!   with optional **drag-to-resize** on their trailing edges. Sizes are sparse
//!   per-index overrides on top of a default.
//! * **1px gridlines** — the viewport background shows through a 1px inset on each
//!   cell's right/bottom edge.
//!
//! You drive it with three callbacks that return plain [`Element`]s — [`cell`],
//! [`col_header`], [`row_header`] — so cells can hold text, an image, a button, an
//! editor, anything. State (scroll/overrides/zoom) is self-managed in signals
//! keyed under `name`. The grid **sizes and positions** whatever you return
//! (return a container, not a bare text node, so the height is honoured), sets a
//! default background if you left it unset, and assigns a stable id
//! (`{name}-c-{r}-{c}`, `{name}-ch-{c}`, `{name}-rh-{r}`) for hit-testing / the
//! agent.
//!
//! [`cell`]: Grid::cell
//! [`col_header`]: Grid::col_header
//! [`row_header`]: Grid::row_header

use std::rc::Rc;

use lumen_core::events::Modifiers;
use lumen_core::state::{Runtime, Signal};
use lumen_core::Color;
use lumen_layout::{Dim, Edges, LayoutStyle, Position};

use crate::{BuildCx, Element};

// --- resizable axis: a default size + sparse per-index overrides (content units)

fn size_of(over: &[(u32, f64)], def: f64, i: u32) -> f64 {
    over.binary_search_by_key(&i, |&(k, _)| k)
        .map(|j| over[j].1)
        .unwrap_or(def)
}

/// Start offset of index `i` = Σ size(k) for k < i.
fn pos_of(over: &[(u32, f64)], def: f64, i: u32) -> f64 {
    let corr: f64 = over
        .iter()
        .take_while(|(k, _)| *k < i)
        .map(|(_, w)| w - def)
        .sum();
    i as f64 * def + corr
}

/// Largest index whose start offset is `<= x` (binary search over `pos_of`).
/// `lo`/`hi` are `u64` so the midpoint can't overflow at the top of `u32`.
fn index_at(over: &[(u32, f64)], def: f64, x: f64) -> u32 {
    let (mut lo, mut hi) = (0u64, u32::MAX as u64);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if pos_of(over, def, mid as u32) <= x {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo as u32
}

fn set_override(over: &mut Vec<(u32, f64)>, i: u32, w: f64) {
    match over.binary_search_by_key(&i, |&(k, _)| k) {
        Ok(j) => over[j].1 = w,
        Err(j) => over.insert(j, (i, w)),
    }
}

/// The cell being built, passed to the [`Grid::cell`] callback. `zoom` is the
/// current zoom factor so content can scale its own font to match the geometry.
#[derive(Clone, Copy, Debug)]
pub struct CellRef {
    /// Row index.
    pub row: u32,
    /// Column index.
    pub col: u32,
    /// Current zoom factor (scale content font by this to match the geometry).
    pub zoom: f64,
}

/// Colours and metrics for a [`Grid`]. `Default` is a dark theme.
#[derive(Clone, Debug)]
pub struct GridStyle {
    /// Gridline colour — the viewport background, shown through the cell inset.
    pub gridline: Color,
    /// Default cell background (used when a cell element leaves `background` unset).
    pub cell: Color,
    /// Header background.
    pub header: Color,
    /// Frozen-corner background.
    pub corner: Color,
    /// Scrollbar track.
    pub track: Color,
    /// Scrollbar thumb.
    pub thumb: Color,
    /// Gridline thickness (each cell is inset this much on its right/bottom edge).
    pub gridline_px: f64,
    /// Scrollbar thickness.
    pub scrollbar_px: f64,
    /// Minimum zoom (only relevant when [`Grid::zoomable`]).
    pub zoom_min: f64,
    /// Maximum zoom (only relevant when [`Grid::zoomable`]).
    pub zoom_max: f64,
}

impl Default for GridStyle {
    fn default() -> GridStyle {
        GridStyle {
            gridline: Color::srgb8(0x2c, 0x34, 0x44, 0xff),
            cell: Color::srgb8(0x16, 0x19, 0x22, 0xff),
            header: Color::srgb8(0x23, 0x2c, 0x3e, 0xff),
            corner: Color::srgb8(0x2a, 0x33, 0x46, 0xff),
            track: Color::srgb8(0x1b, 0x20, 0x2b, 0xff),
            thumb: Color::srgb8(0x45, 0x50, 0x68, 0xff),
            gridline_px: 1.0,
            scrollbar_px: 12.0,
            zoom_min: 0.6,
            zoom_max: 2.5,
        }
    }
}

type CellFn = Rc<dyn Fn(&BuildCx, CellRef) -> Option<Element>>;
type HeaderFn = Rc<dyn Fn(u32) -> Element>;

/// A virtualized 2D grid. Build it up, then [`build`](Grid::build) it into an
/// [`Element`] that fills its container. See the [module docs](self).
/// # Example
///
/// ```
/// use lumen_widgets::{widgets, App, Grid};
///
/// let app = App::new(|cx| {
///     Grid::new("g", 2, 3, 48.0, 32.0)
///         .cell(|_, c| Some(widgets::text(format!("{},{}", c.row, c.col))))
///         .build(cx)
/// });
/// # lumen_widgets::doc_shot(app, 180.0, 100.0, "grid");
/// ```
///
/// Renders:
///
/// ![Grid example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/grid.png)
///
/// The picture above is `src/doc_shots/grid.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Grid {
    name: String,
    rows: u32,
    cols: u32,
    def_w: f64,
    def_h: f64,
    header_w: f64, // left row-header column width (0 = none)
    header_h: f64, // top column-header row height (0 = none)
    col_header: Option<HeaderFn>,
    row_header: Option<HeaderFn>,
    cell: CellFn,
    resizable: bool,
    zoomable: bool,
    scrollbars: bool,
    extent: (u32, u32),
    // Window-space rect the grid occupies (drags arrive in window coords). Defaults
    // to the whole surface; set it when the grid isn't the whole window.
    viewport: Option<(f64, f64, f64, f64)>,
    style: GridStyle,
}

impl Grid {
    /// A grid `rows × cols` with default cell size `def_w × def_h` (content px).
    /// Use `u32::MAX` for an effectively unbounded axis.
    pub fn new(name: impl Into<String>, rows: u32, cols: u32, def_w: f64, def_h: f64) -> Grid {
        Grid {
            name: name.into(),
            rows,
            cols,
            def_w,
            def_h,
            header_w: 0.0,
            header_h: 0.0,
            col_header: None,
            row_header: None,
            cell: Rc::new(|_, _| None),
            resizable: false,
            zoomable: false,
            scrollbars: true,
            extent: (rows, cols),
            viewport: None,
            style: GridStyle::default(),
        }
    }

    /// The content of cell `(row, col)`, or `None` for an empty cell. The grid
    /// sizes/positions and (if `background` is unset) fills whatever you return.
    pub fn cell(mut self, f: impl Fn(&BuildCx, CellRef) -> Option<Element> + 'static) -> Grid {
        self.cell = Rc::new(f);
        self
    }

    /// Add a frozen top header row of `height`, its content built per column.
    pub fn col_header(mut self, height: f64, f: impl Fn(u32) -> Element + 'static) -> Grid {
        self.header_h = height;
        self.col_header = Some(Rc::new(f));
        self
    }

    /// Add a frozen left header column of `width`, its content built per row.
    pub fn row_header(mut self, width: f64, f: impl Fn(u32) -> Element + 'static) -> Grid {
        self.header_w = width;
        self.row_header = Some(Rc::new(f));
        self
    }

    /// Enable drag-to-resize on header trailing edges (needs the matching header).
    pub fn resizable(mut self, on: bool) -> Grid {
        self.resizable = on;
        self
    }

    /// Enable Ctrl/Cmd + wheel zoom.
    pub fn zoomable(mut self, on: bool) -> Grid {
        self.zoomable = on;
        self
    }

    /// Show draggable scrollbars (default on).
    pub fn scrollbars(mut self, on: bool) -> Grid {
        self.scrollbars = on;
        self
    }

    /// The nominal `(rows, cols)` the scrollbar thumbs represent (the "used
    /// range"). Defaults to the full dimensions; set a smaller hint for an
    /// unbounded grid so the thumb stays meaningful. The thumb still grows to
    /// include the current scroll position.
    pub fn extent(mut self, rows: u32, cols: u32) -> Grid {
        self.extent = (rows, cols);
        self
    }

    /// The grid's window-space rect `(x, y, w, h)`. Drags arrive in window
    /// coordinates, and virtualization needs the size (layout runs *after* build),
    /// so set this when the grid isn't the whole window (e.g. below a toolbar).
    /// Defaults to `(0, 0, surface_w, surface_h)`.
    pub fn viewport(mut self, x: f64, y: f64, w: f64, h: f64) -> Grid {
        self.viewport = Some((x, y, w, h));
        self
    }

    /// Override the colours/metrics.
    pub fn style(mut self, style: GridStyle) -> Grid {
        self.style = style;
        self
    }

    /// The current zoom of the grid named `name` (1.0 if never zoomed).
    pub fn zoom_of(cx: &BuildCx, name: &str) -> f64 {
        cx.signal(&format!("{name}.zoom"), || 1.0f64)
            .get(cx.runtime())
    }

    /// Build the grid into an [`Element`] (a clipped viewport that fills its
    /// container).
    pub fn build(self, cx: &BuildCx) -> Element {
        let s = &self.style;
        let gp = s.gridline_px;
        let (zmin, zmax) = (s.zoom_min, s.zoom_max);
        let name = &self.name;

        let sx = cx.signal(&format!("{name}.sx"), || 0.0f64);
        let sy = cx.signal(&format!("{name}.sy"), || 0.0f64);
        let cw = cx.signal(&format!("{name}.cw"), Vec::<(u32, f64)>::new);
        let rh = cx.signal(&format!("{name}.rh"), Vec::<(u32, f64)>::new);
        let zoom = cx.signal(&format!("{name}.zoom"), || 1.0f64);

        let z = if self.zoomable {
            zoom.get(cx.runtime()).clamp(zmin, zmax)
        } else {
            1.0
        };
        let (ox, oy) = (sx.get(cx.runtime()), sy.get(cx.runtime()));
        let cwv = cw.get(cx.runtime());
        let rhv = rh.get(cx.runtime());
        let (def_w, def_h) = (self.def_w, self.def_h);
        let (hw, hh) = (self.header_w, self.header_h);

        // Window rect → content viewport (minus the frozen header band), in
        // content units (÷ zoom).
        let (vx, vy, vpw, vph) = self
            .viewport
            .unwrap_or_else(|| (0.0, 0.0, cx.size().width, cx.size().height));
        let vw = (vpw - hw).max(0.0);
        let vh = (vph - hh).max(0.0);
        let (vwc, vhc) = (vw / z, vh / z);

        // Visible index ranges (+1 row/col of overscan each side), clamped to dims.
        let c0 = index_at(&cwv, def_w, ox).saturating_sub(1);
        let c1 = index_at(&cwv, def_w, ox + vwc)
            .saturating_add(2)
            .min(self.cols);
        let r0 = index_at(&rhv, def_h, oy).saturating_sub(1);
        let r1 = index_at(&rhv, def_h, oy + vhc)
            .saturating_add(2)
            .min(self.rows);

        // Content → screen (headers occupy the top-left frozen band; zoom scales).
        let cx_of = |c: u32| hw + (pos_of(&cwv, def_w, c) - ox) * z;
        let ry_of = |r: u32| hh + (pos_of(&rhv, def_h, r) - oy) * z;
        let eff_w = |c: u32| size_of(&cwv, def_w, c) * z;
        let eff_h = |r: u32| size_of(&rhv, def_h, r) * z;

        let mut layers: Vec<Element> = Vec::new();

        // Cells — the grid sizes/positions and (if unset) fills + ids each one.
        for r in r0..r1 {
            let (y, h) = (ry_of(r), eff_h(r));
            for c in c0..c1 {
                if let Some(mut el) = (self.cell)(
                    cx,
                    CellRef {
                        row: r,
                        col: c,
                        zoom: z,
                    },
                ) {
                    place(&mut el, cx_of(c), y, eff_w(c), h, gp);
                    if el.background.is_none() {
                        el.background = Some(s.cell);
                    }
                    if el.id.is_none() {
                        el.id = Some(format!("{name}-c-{r}-{c}").into());
                    }
                    layers.push(el);
                }
            }
        }

        // Column headers.
        if let Some(hf) = &self.col_header {
            for c in c0..c1 {
                let mut el = hf(c);
                place(&mut el, cx_of(c), 0.0, eff_w(c), hh, gp);
                if el.background.is_none() {
                    el.background = Some(s.header);
                }
                el.id = Some(format!("{name}-ch-{c}").into());
                layers.push(el);
            }
        }
        // Row headers.
        if let Some(hf) = &self.row_header {
            for r in r0..r1 {
                let mut el = hf(r);
                place(&mut el, 0.0, ry_of(r), hw, eff_h(r), gp);
                if el.background.is_none() {
                    el.background = Some(s.header);
                }
                el.id = Some(format!("{name}-rh-{r}").into());
                layers.push(el);
            }
        }

        // Resize handles last, so they win hit-testing over the neighbouring header.
        if self.resizable && self.col_header.is_some() {
            for c in c0..c1 {
                layers.push(col_handle(
                    name,
                    c,
                    pos_of(&cwv, def_w, c),
                    cx_of(c) + eff_w(c),
                    hh,
                    hw,
                    vx,
                    (zmin, zmax),
                    self.zoomable,
                    sx,
                    zoom,
                    cw,
                ));
            }
        }
        if self.resizable && self.row_header.is_some() {
            for r in r0..r1 {
                layers.push(row_handle(
                    name,
                    r,
                    pos_of(&rhv, def_h, r),
                    ry_of(r) + eff_h(r),
                    hw,
                    hh,
                    vy,
                    (zmin, zmax),
                    self.zoomable,
                    sy,
                    zoom,
                    rh,
                ));
            }
        }

        // Frozen top-left corner.
        if hw > 0.0 && hh > 0.0 {
            layers.push(filled(0.0, 0.0, hw, hh, s.corner, gp));
        }

        // Scrollbars — extent = the used range, grown to include where we are.
        if self.scrollbars {
            let content_h = pos_of(&rhv, def_h, self.extent.0).max(oy + vhc);
            let content_w = pos_of(&cwv, def_w, self.extent.1).max(ox + vwc);
            if vh > 0.0 {
                layers.push(vscrollbar(name, vpw, vh, hh, content_h, oy, vhc, vy, s, sy));
            }
            if vw > 0.0 {
                layers.push(hscrollbar(name, vph, vw, hw, content_w, ox, vwc, vx, s, sx));
            }
        }

        // The clipped viewport: its gridline-coloured background shows through the
        // 1px cell insets. It owns the wheel handler.
        let mut viewport = Element {
            role: lumen_core::semantics::Role::Group,
            style: LayoutStyle {
                position: Position::Relative,
                width: Dim::pct(1.0),
                flex_grow: 1.0,
                min_height: Dim::px(0.0),
                ..LayoutStyle::default()
            },
            clip: true,
            children: layers,
            ..Element::default()
        };
        viewport.background = Some(s.gridline);
        let zoomable = self.zoomable;
        viewport.on_wheel = Some(Rc::new(move |rt, dx, dy, mods| {
            if zoomable && (mods.contains(Modifiers::CTRL) || mods.contains(Modifiers::META)) {
                zoom.update(rt, |zz| *zz = (*zz * (1.0 - dy * 0.0016)).clamp(zmin, zmax));
            } else if mods.contains(Modifiers::SHIFT) {
                let zz = zclamp(zoom, rt, zoomable, zmin, zmax);
                sx.update(rt, |o| *o = (*o + (dy + dx) / zz).max(0.0));
            } else {
                let zz = zclamp(zoom, rt, zoomable, zmin, zmax);
                sy.update(rt, |o| *o = (*o + dy / zz).max(0.0));
                sx.update(rt, |o| *o = (*o + dx / zz).max(0.0));
            }
        }));
        viewport
    }
}

fn zclamp(zoom: Signal<f64>, rt: &Runtime, zoomable: bool, zmin: f64, zmax: f64) -> f64 {
    if zoomable {
        zoom.get(rt).clamp(zmin, zmax)
    } else {
        1.0
    }
}

/// Set an element's absolute position + size (with the gridline inset on the
/// right/bottom edges), clearing any min-size so content can't overflow the cell.
fn place(el: &mut Element, x: f64, y: f64, w: f64, h: f64, gp: f64) {
    el.style.position = Position::Absolute;
    el.style.inset = Edges {
        left: Dim::px(x as f32),
        top: Dim::px(y as f32),
        ..Edges::AUTO
    };
    el.style.width = Dim::px((w - gp).max(0.0) as f32);
    el.style.height = Dim::px((h - gp).max(0.0) as f32);
    el.style.min_width = Dim::px(0.0);
    el.style.min_height = Dim::px(0.0);
}

/// A plain filled box at `(x, y)` sized `(w, h)`, inset by `gp` on r/b edges.
fn filled(x: f64, y: f64, w: f64, h: f64, bg: Color, gp: f64) -> Element {
    let mut e = Element::default();
    e.style.position = Position::Absolute;
    e.style.inset = Edges {
        left: Dim::px(x as f32),
        top: Dim::px(y as f32),
        ..Edges::AUTO
    };
    e.style.width = Dim::px((w - gp).max(0.0) as f32);
    e.style.height = Dim::px((h - gp).max(0.0) as f32);
    e.background = Some(bg);
    e
}

/// An invisible, hit-testable strip centred on a column's right border; dragging
/// it resizes that column (mapping the pointer to content x via the viewport
/// origin + zoom).
#[allow(clippy::too_many_arguments)]
fn col_handle(
    name: &str,
    c: u32,
    left_content: f64,
    border_x: f64,
    hh: f64,
    hw: f64,
    vx: f64,
    zc: (f64, f64),
    zoomable: bool,
    sx: Signal<f64>,
    zoom: Signal<f64>,
    cw: Signal<Vec<(u32, f64)>>,
) -> Element {
    let mut e = strip(border_x - 3.5, 0.0, 7.0, hh);
    e.id = Some(format!("{name}-cx-{c}").into());
    e.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let z = zclamp(zoom, rt, zoomable, zc.0, zc.1);
        let content_x = sx.get(rt) + (pos.x - vx - hw) / z;
        let neww = (content_x - left_content).clamp(24.0, 400.0);
        cw.update(rt, move |v| set_override(v, c, neww));
    }));
    e
}

/// An invisible, hit-testable strip centred on a row's bottom border; dragging it
/// resizes that row.
#[allow(clippy::too_many_arguments)]
fn row_handle(
    name: &str,
    r: u32,
    top_content: f64,
    border_y: f64,
    hw: f64,
    hh: f64,
    vy: f64,
    zc: (f64, f64),
    zoomable: bool,
    sy: Signal<f64>,
    zoom: Signal<f64>,
    rh: Signal<Vec<(u32, f64)>>,
) -> Element {
    let mut e = strip(0.0, border_y - 3.5, hw, 7.0);
    e.id = Some(format!("{name}-ry-{r}").into());
    e.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let z = zclamp(zoom, rt, zoomable, zc.0, zc.1);
        let content_y = sy.get(rt) + (pos.y - vy - hh) / z;
        let newh = (content_y - top_content).clamp(16.0, 240.0);
        rh.update(rt, move |v| set_override(v, r, newh));
    }));
    e
}

/// An invisible absolutely-positioned box (a drag handle).
fn strip(x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut e = Element::default();
    e.style.position = Position::Absolute;
    e.style.inset = Edges {
        left: Dim::px(x as f32),
        top: Dim::px(y as f32),
        ..Edges::AUTO
    };
    e.style.width = Dim::px(w as f32);
    e.style.height = Dim::px(h as f32);
    e
}

/// Vertical scrollbar down the right edge (a track + draggable thumb). Sizes are
/// screen px; extents are content units.
#[allow(clippy::too_many_arguments)]
fn vscrollbar(
    name: &str,
    vpw: f64,
    track_h: f64,
    hh: f64,
    content_h: f64,
    oy: f64,
    vhc: f64,
    vy: f64,
    s: &GridStyle,
    sy: Signal<f64>,
) -> Element {
    let sb = s.scrollbar_px;
    let thumb_h = ((vhc / content_h).clamp(0.06, 1.0) * track_h).max(24.0);
    let span = content_h - vhc;
    let pos_frac = if span > 0.0 {
        (oy / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let travel = track_h - thumb_h;

    let mut track = filled(vpw - sb, hh, sb, track_h, s.track, 0.0);
    let mut thumb = strip(2.0, pos_frac * travel, sb - 4.0, thumb_h);
    thumb.background = Some(s.thumb);
    thumb.corner_radius = (sb - 4.0) / 2.0;
    thumb.id = Some(format!("{name}-vthumb").into());
    thumb.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let frac = if travel > 0.0 {
            ((pos.y - vy - hh) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        sy.set(rt, (frac * span).max(0.0));
    }));
    track.children.push(thumb);
    track
}

/// Horizontal scrollbar along the bottom edge.
#[allow(clippy::too_many_arguments)]
fn hscrollbar(
    name: &str,
    vph: f64,
    track_w: f64,
    hw: f64,
    content_w: f64,
    ox: f64,
    vwc: f64,
    vx: f64,
    s: &GridStyle,
    sx: Signal<f64>,
) -> Element {
    let sb = s.scrollbar_px;
    let thumb_w = ((vwc / content_w).clamp(0.06, 1.0) * track_w).max(24.0);
    let span = content_w - vwc;
    let pos_frac = if span > 0.0 {
        (ox / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let travel = track_w - thumb_w;

    let mut track = filled(hw, vph - sb, track_w, sb, s.track, 0.0);
    let mut thumb = strip(pos_frac * travel, 2.0, thumb_w, sb - 4.0);
    thumb.background = Some(s.thumb);
    thumb.corner_radius = (sb - 4.0) / 2.0;
    thumb.id = Some(format!("{name}-hthumb").into());
    thumb.on_drag = Some(Rc::new(move |rt, _fx, _fy, pos| {
        let frac = if travel > 0.0 {
            ((pos.x - vx - hw) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        sx.set(rt, (frac * span).max(0.0));
    }));
    track.children.push(thumb);
    track
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use lumen_core::events::{Event, Modifiers, WheelEvent};
    use lumen_core::geometry::{Point, Size, Vec2};

    /// A 100k × 100k grid materializes only its viewport, and the wheel scrolls
    /// its namespaced offset — the reusable mechanics, with empty cell content.
    #[test]
    fn virtualizes_and_scrolls() {
        let mut h = App::new(|cx| {
            Grid::new("g", 100_000, 100_000, 40.0, 20.0)
                .col_header(20.0, |_| Element::default())
                .row_header(30.0, |_| Element::default())
                .resizable(true)
                .zoomable(true)
                .cell(|_cx, _cell| Some(Element::default()))
                .build(cx)
        })
        .run_headless(Size::new(400.0, 300.0));

        let n = h.pump().node_count;
        assert!(n < 900, "virtualized over 10^10 cells, got {n}");
        h.assert_view_coherent();

        h.inject(Event::Wheel(WheelEvent {
            pos: Point::new(200.0, 200.0),
            delta: Vec2::new(0.0, 500.0),
            modifiers: Modifiers::empty(),
        }));
        h.pump();
        let sy: lumen_core::state::Signal<f64> = h.runtime().signal("g.sy", || 0.0);
        assert!(sy.get(h.runtime()) > 100.0, "wheel scrolled the grid");
        assert!(h.pump().node_count < 900, "still bounded after scroll");
        h.assert_view_coherent();
    }
}
