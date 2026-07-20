//! system_information — a small monitor dashboard: live OS/arch/CPU facts as
//! big-number stat tiles. Values come from `system_info()`; chrome from
//! `app.lss`.
use lumen_widgets::element::Shadow;
use lumen_widgets::system::system_info;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

/// Build the system-information app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// One stat tile: a small muted label above a big value.
fn tile(label: &str, value: &str, accent: bool, id: &str) -> Element {
    let value_class = if accent { "stat-accent" } else { "stat-value" };
    let mut c = widgets::column(vec![
        txt(label, 11.0, 800.0).class("stat-label"),
        txt(value, 26.0, 800.0).class(value_class),
    ])
    .class("tile")
    .id(id);
    c.style.row_gap = Dim::px(6.0);
    c.style.padding = Edges::all(Dim::px(18.0));
    c.style.width = Dim::px(190.0);
    c.style.align_items = Some(Align::Start);
    c
}

/// The "live" status pill: an accent dot beside an uppercase label.
fn status() -> Element {
    let mut dot = Element::default().class("dot");
    dot.style.width = Dim::px(8.0);
    dot.style.height = Dim::px(8.0);
    let mut r = widgets::row(vec![dot, txt("ONLINE", 11.0, 800.0).class("status")]);
    r.style.column_gap = Dim::px(7.0);
    r.style.align_items = Some(Align::Center);
    r
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let info = system_info();

    let header = {
        let mut r = widgets::row(vec![
            txt("System information", 24.0, 800.0).class("title"),
            status(),
        ]);
        r.style.column_gap = Dim::px(14.0);
        r.style.align_items = Some(Align::Center);
        r.style.justify_content = Some(Align::SpaceBetween);
        r.style.width = Dim::pct(1.0);
        r
    };

    let grid = {
        let mut top = widgets::row(vec![
            tile("OPERATING SYSTEM", &info.os, false, "os"),
            tile("ARCHITECTURE", &info.arch, false, "arch"),
        ]);
        top.style.column_gap = Dim::px(14.0);
        // M.6: richer host facts (opt-in `sysinfo` feature) — the default
        // build stays dependency-free via `system_info()`.
        #[cfg(feature = "sysinfo")]
        let mem = {
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            format!(
                "{:.1} GiB",
                sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0)
            )
        };
        #[cfg(not(feature = "sysinfo"))]
        let mem = "enable `sysinfo`".to_string();
        let mut bottom = widgets::row(vec![
            tile("LOGICAL CPUS", &info.cpus.to_string(), true, "cpus"),
            tile("MEMORY", &mem, false, "memory"),
            tile("RENDERER", "tiny-skia", false, "renderer"),
        ]);
        bottom.style.column_gap = Dim::px(14.0);
        let mut c = widgets::column(vec![top, bottom]);
        c.style.row_gap = Dim::px(14.0);
        c
    };

    let mut card = widgets::column(vec![
        header,
        txt("Reported by the host at build time.", 14.0, 400.0).class("subtitle"),
        grid,
    ])
    .id("card");
    card.style.align_items = Some(Align::Start);
    card.style.row_gap = Dim::px(18.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
