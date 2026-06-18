//! PULSE — a luminous chronograph stopwatch, standalone Lumen example.
//!
//! The hero is a circular dial: a 60-tick bezel, a recessed face, and a glowing
//! progress arc that sweeps once per minute, with an oversized tabular readout
//! dead-centre. Two themes — "Eclipse" (dark) and "Daybreak" (light) — toggle
//! live. The control colours were tuned against the design-analysis APCA
//! contrast tool (`Headless::contrast_report`) so every label is legible.
//!
//! It runs off the virtual clock: while running it integrates elapsed time each
//! frame and calls `cx.animate()` so the arc and readout tick on their own.

use kurbo::{Arc, BezPath, Circle, Point, Shape, Vec2};
use lumen_core::geometry::Size;
use lumen_core::{Color, Runtime};
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_render::canvas::Frame;
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the PULSE stopwatch app (starts in the dark "Eclipse" theme).
pub fn main_app() -> App {
    App::new(build)
}

// --- palette ----------------------------------------------------------------

fn c(hex: &str) -> Color {
    Color::from_hex(hex).expect("valid hex")
}

/// Every colour the design uses, so the two themes are declared in one place.
#[derive(Clone, Copy)]
struct Pal {
    page: Color,
    surface: Color,
    shadow: Color,
    track: Color,
    tick_minor: Color,
    tick_major: Color,
    disc: Color,
    arc: Color,
    head: Color,
    head_halo: Color,
    readout: Color,
    status: Color,
    accent: Color,
    brand: Color,
    primary_fill: Color,
    primary_text: Color,
    stop_fill: Color,
    stop_text: Color,
    ghost_fill: Color,
    ghost_text: Color,
}

fn dark() -> Pal {
    Pal {
        page: c("#0c0f15"),
        surface: c("#161b24"),
        shadow: c("#00000088"),
        track: c("#232b39"),
        tick_minor: c("#2b3543"),
        tick_major: c("#46546b"),
        disc: c("#10141c"),
        arc: c("#2dd4bf"),
        head: c("#5eead4"),
        head_halo: c("#2dd4bf55"),
        readout: c("#eef3f8"),
        status: c("#b3bdcc"),
        accent: c("#2dd4bf"),
        brand: c("#2dd4bf"),
        primary_fill: c("#2dd4bf"),
        primary_text: c("#03110e"),
        stop_fill: c("#e11d48"),
        stop_text: c("#fff1f3"),
        ghost_fill: c("#1c2330"),
        ghost_text: c("#c4cddb"),
    }
}

fn light() -> Pal {
    Pal {
        page: c("#f5f1e8"),
        surface: c("#fffdf8"),
        shadow: c("#2b231a33"),
        track: c("#e7dfd0"),
        tick_minor: c("#d9d0bf"),
        tick_major: c("#b7a98f"),
        disc: c("#f6f0e4"),
        arc: c("#e8590c"),
        head: c("#ff7a33"),
        head_halo: c("#e8590c44"),
        readout: c("#1c1d22"),
        status: c("#756b58"),
        accent: c("#c2490a"),
        brand: c("#b8480a"),
        primary_fill: c("#1c1d22"),
        primary_text: c("#f7f3ea"),
        stop_fill: c("#b8480a"),
        stop_text: c("#fff4ec"),
        ghost_fill: c("#efe8da"),
        ghost_text: c("#44403a"),
    }
}

// --- build ------------------------------------------------------------------

fn build(cx: &mut BuildCx) -> Element {
    let elapsed = cx.signal("elapsed_ms", || 0.0f64);
    let running = cx.signal("running", || false);
    let last = cx.signal("last_ms", || 0.0f64);
    let is_dark = cx.signal("dark", || true);
    let rt = cx.runtime();

    // Integrate elapsed time while running (handlers can't read the clock).
    let now = cx.now_ms();
    let on = running.get(rt);
    let prev = last.get(rt);
    if on {
        elapsed.update(rt, |e| *e += (now - prev).max(0.0));
        cx.animate();
    }
    last.set(rt, now);

    let pal = if is_dark.get(rt) { dark() } else { light() };
    let ms = elapsed.get(rt);
    let total_s = (ms / 1000.0) as i64;
    let mm = total_s / 60;
    let ss = total_s % 60;
    let cs = ((ms % 1000.0) / 10.0) as i64;
    let frac = (ms / 1000.0 % 60.0) / 60.0;
    let status = if on {
        "RUNNING"
    } else if ms > 0.0 {
        "PAUSED"
    } else {
        "READY"
    };

    let readout = format!("{mm:02}:{ss:02}");
    let centis = format!(".{cs:02}");

    // --- header: brand + theme toggle ---
    let header = Element {
        role: lumen_core::semantics::Role::Group,
        elide_semantics: true,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            width: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::SpaceBetween),
            ..LayoutStyle::default()
        },
        children: vec![
            styled_text("PULSE", 15.0, 700.0, pal.brand),
            theme_toggle(is_dark.get(rt), pal, move |rt| {
                is_dark.update(rt, |d| *d = !*d)
            }),
        ],
        ..Element::default()
    };

    // --- dial: canvas face + centred readout overlay ---
    let dial = dial(frac, pal, status, &readout, &centis);

    // --- controls ---
    let (toggle_label, toggle_fill, toggle_text) = if on {
        ("Stop", pal.stop_fill, pal.stop_text)
    } else {
        ("Start", pal.primary_fill, pal.primary_text)
    };
    let toggle_btn = pill(toggle_label, toggle_fill, toggle_text, move |rt| {
        running.update(rt, |r| *r = !*r)
    })
    .id("toggle");
    let reset_btn = pill("Reset", pal.ghost_fill, pal.ghost_text, move |rt| {
        elapsed.set(rt, 0.0);
        running.set(rt, false);
    })
    .id("reset");

    let controls = Element {
        role: lumen_core::semantics::Role::Group,
        elide_semantics: true,
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            width: Dim::pct(1.0),
            column_gap: Dim::px(12.0),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![toggle_btn, reset_btn],
        ..Element::default()
    };

    // --- device body ---
    let body = Element {
        role: lumen_core::semantics::Role::Group,
        background: Some(pal.surface),
        corner_radius: 30.0,
        shadow: Some(Shadow {
            dx: 0.0,
            dy: 22.0,
            blur: 48.0,
            spread: 0.0,
            color: pal.shadow,
        }),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::px(360.0),
            height: Dim::px(468.0),
            padding: Edges::all(Dim::px(28.0)),
            row_gap: Dim::px(20.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![header, dial, controls],
        ..Element::default()
    };

    // --- full-window page ---
    Element {
        role: lumen_core::semantics::Role::Group,
        background: Some(pal.page),
        style: LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dim::pct(1.0),
            height: Dim::pct(1.0),
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            ..LayoutStyle::default()
        },
        children: vec![body],
        ..Element::default()
    }
}

// --- dial -------------------------------------------------------------------

const DIAL: f64 = 280.0;

fn dial(frac: f64, pal: Pal, status: &str, readout: &str, centis: &str) -> Element {
    let face = widgets::canvas(DIAL, DIAL, move |f, _s| draw_dial(f, frac, pal));

    let overlay = Element {
        role: lumen_core::semantics::Role::Group,
        elide_semantics: true,
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges::all(Dim::px(0.0)),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            row_gap: Dim::px(2.0),
            ..LayoutStyle::default()
        },
        children: vec![
            styled_text(status, 13.0, 600.0, pal.status),
            styled_text(readout, 66.0, 800.0, pal.readout),
            styled_text(centis, 22.0, 600.0, pal.accent),
        ],
        ..Element::default()
    };

    Element {
        role: lumen_core::semantics::Role::Group,
        elide_semantics: true,
        style: LayoutStyle {
            position: Position::Relative,
            width: Dim::px(DIAL as f32),
            height: Dim::px(DIAL as f32),
            ..LayoutStyle::default()
        },
        children: vec![face, overlay],
        ..Element::default()
    }
}

/// Paint the chronograph face: recessed disc, bezel track, 60 ticks, the
/// luminous progress arc, and a glowing head.
fn draw_dial(f: &mut Frame, frac: f64, pal: Pal) {
    let center = Point::new(DIAL / 2.0, DIAL / 2.0);
    let r_track = DIAL / 2.0 - 26.0;
    let r_disc = r_track - 14.0;

    // Recessed face.
    f.fill_circle(center, r_disc, pal.disc);

    // Bezel track ring.
    let track = Circle::new(center, r_track).to_path(0.1);
    f.stroke(&track, pal.track, 10.0);

    // 60 ticks (every 5th is a longer, brighter "major").
    let tick_out = r_track - 9.0;
    let mut minor = BezPath::new();
    let mut major = BezPath::new();
    for i in 0..60 {
        let a = -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::TAU / 60.0;
        let (ca, sa) = (a.cos(), a.sin());
        let is_major = i % 5 == 0;
        let len = if is_major { 11.0 } else { 6.0 };
        let p_out = Point::new(center.x + ca * tick_out, center.y + sa * tick_out);
        let p_in = Point::new(
            center.x + ca * (tick_out - len),
            center.y + sa * (tick_out - len),
        );
        let path = if is_major { &mut major } else { &mut minor };
        path.move_to(p_out);
        path.line_to(p_in);
    }
    f.stroke(&minor, pal.tick_minor, 2.0);
    f.stroke(&major, pal.tick_major, 3.0);

    // Progress arc — sweeps clockwise from 12 o'clock.
    if frac > 0.0001 {
        let arc = Arc::new(
            center,
            Vec2::new(r_track, r_track),
            -std::f64::consts::FRAC_PI_2,
            frac * std::f64::consts::TAU,
            0.0,
        )
        .to_path(0.1);
        f.stroke(&arc, pal.arc, 10.0);
    }

    // Glowing head at the arc tip.
    let ha = -std::f64::consts::FRAC_PI_2 + frac * std::f64::consts::TAU;
    let hp = Point::new(center.x + ha.cos() * r_track, center.y + ha.sin() * r_track);
    f.fill_circle(hp, 11.0, pal.head_halo);
    f.fill_circle(hp, 6.0, pal.head);
}

// --- small helpers ----------------------------------------------------------

fn styled_text(s: impl Into<String>, size: f32, weight: f32, color: Color) -> Element {
    let mut el = widgets::text(s);
    if let Some((_, ts)) = &mut el.text {
        ts.font_size = size;
        ts.weight = weight;
        ts.color = color;
    }
    el
}

fn pill(
    label: impl Into<String>,
    fill: Color,
    text: Color,
    on_click: impl Fn(&Runtime) + 'static,
) -> Element {
    // The label is a centred *child* (a flex item), because the renderer paints
    // a node's own `text` at its box origin (padding doesn't recentre it).
    let label = label.into();
    Element {
        role: lumen_core::semantics::Role::Button,
        label: label.clone(),
        focusable: true,
        actions: vec![
            lumen_core::semantics::Action::Click,
            lumen_core::semantics::Action::Focus,
        ],
        on_click: Some(std::rc::Rc::new(on_click)),
        background: Some(fill),
        corner_radius: 13.0,
        style: LayoutStyle {
            display: Display::Flex,
            align_items: Some(Align::Center),
            justify_content: Some(Align::Center),
            padding: Edges {
                left: Dim::px(30.0),
                right: Dim::px(30.0),
                top: Dim::px(14.0),
                bottom: Dim::px(14.0),
            },
            ..LayoutStyle::default()
        },
        children: vec![styled_text(label, 16.0, 600.0, text)],
        ..Element::default()
    }
}

fn theme_toggle(is_dark: bool, pal: Pal, on_click: impl Fn(&Runtime) + 'static) -> Element {
    let label = if is_dark { "LIGHT" } else { "DARK" };
    let mut el = widgets::button(label, on_click);
    el.background = Some(pal.ghost_fill);
    el.corner_radius = 11.0;
    el.style.padding = Edges {
        left: Dim::px(14.0),
        right: Dim::px(14.0),
        top: Dim::px(8.0),
        bottom: Dim::px(8.0),
    };
    if let Some((_, ts)) = &mut el.text {
        ts.color = pal.ghost_text;
        ts.weight = 600.0;
        ts.font_size = 12.0;
    }
    el.id("theme")
}

/// Build a headless instance at a comfortable size for the device body.
pub fn run_headless() -> lumen_widgets::Headless {
    main_app().run_headless(Size::new(440.0, 580.0))
}
