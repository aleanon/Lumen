//! The **agent gauntlet** app (M4-exit): a multi-screen, `.lss`-styled UI with
//! one custom shader and a deliberately injectable layout bug. The release-gate
//! driver in `tests/gauntlet.rs` scaffolds → builds → verifies on desktop +
//! mobile → exports a test from its own session → detects and fixes the layout
//! bug via structured diagnostics, all through the CLI + lumen-agent.

use lumen_render::RgbaImage;
use lumen_widgets::shader::ShaderWidget;
use lumen_widgets::{widgets, widgets_m1, widgets_m4, App, BuildCx, Element};

/// The app stylesheet (themeable; hot-reloadable).
pub const STYLESHEET: &str = r#"
@tokens { accent: #1a73e8ff; }
#title { color: $accent; }
"#;

/// The one custom fragment shader the gauntlet renders.
pub const SHADER: &str = "@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { let v = 0.5 + 0.5 * sin((uv.x + uv.y) * 12.0); return vec4<f32>(uv.x, v, uv.y, 1.0); }";

fn render_shader() -> RgbaImage {
    let mut w = ShaderWidget::new(96, 96, lumen_core::Color::srgb8(0x1a, 0x73, 0xe8, 0xff));
    w.set_source(SHADER);
    w.image().clone()
}

/// Build the gauntlet application (shader rendered once at construction).
pub fn main_app() -> App {
    let shader = render_shader();
    App::new(move |cx| build(cx, &shader)).stylesheet(STYLESHEET)
}

fn build(cx: &mut BuildCx, shader: &RgbaImage) -> Element {
    let tab = cx.signal("tab", || 0usize);
    let current = tab.get(cx.runtime());
    // The injected layout bug is on by default; the agent fixes it.
    let bug = cx.signal("bug", || true);
    let buggy = bug.get(cx.runtime());

    let header = widgets::row(vec![
        widgets::text("Gauntlet").id("title"),
        widgets_m1::spacer(),
        widgets::button("Fix layout", move |rt| bug.set(rt, false)).id("fix"),
    ]);
    let nav = widgets_m1::tabs(cx, "tab", &["Home", "Shader", "Data"]);

    let screen = match current {
        0 => home(buggy),
        1 => widgets::column(vec![
            widgets::text("Custom shader").id("shader-label"),
            widgets::image(shader.clone()).id("shader"),
        ]),
        _ => widgets::column(vec![
            widgets::text("Data").id("data-label"),
            widgets_m4::data_grid(cx, "grid", &["#", "Name"], 1000, 20.0, 160.0, |r, c| {
                if c == 0 {
                    format!("{r}")
                } else {
                    format!("row {r}")
                }
            }),
        ]),
    };

    widgets::column(vec![header, nav, widgets_m1::divider(), screen]).id("root")
}

fn home(buggy: bool) -> Element {
    let mut kids = vec![widgets::text("Welcome to the gauntlet").id("welcome")];
    if buggy {
        // A fixed-width box whose child is forced wider — overflows (→ W0103).
        kids.push(
            Element {
                role: lumen_core::semantics::Role::Group,
                style: lumen_layout::LayoutStyle {
                    width: lumen_layout::Dim::px(50.0),
                    height: lumen_layout::Dim::px(20.0),
                    ..Default::default()
                },
                children: vec![Element {
                    role: lumen_core::semantics::Role::Text,
                    label: "overflowing content".into(),
                    style: lumen_layout::LayoutStyle {
                        min_width: lumen_layout::Dim::px(200.0),
                        ..Default::default()
                    },
                    text: Some((
                        "overflowing content".into(),
                        lumen_text::TextStyle::default(),
                    )),
                    ..Element::default()
                }
                .id("bug-child")],
                ..Element::default()
            }
            .id("bug-box"),
        );
    }
    widgets::column(kids)
}
