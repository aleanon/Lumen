//! qr — M.6: live QR encoding (pure-Rust `qrcodegen`) drawn through the
//! immediate-mode Canvas: type text, the code re-encodes and repaints.
use lumen_layout::{Align, Dim, Display, FlexDirection, LayoutStyle};
use lumen_widgets::{widgets, App, BuildCx, Element};
use qrcodegen::{QrCode, QrCodeEcc};

/// Build the QR app.
pub fn main_app() -> App {
    App::new(build)
}

/// Encode `text` and draw it as filled module squares.
fn qr_canvas(text: String) -> Element {
    let mut el = widgets::canvas(220.0, 220.0, move |frame, size| {
        let Ok(code) = QrCode::encode_text(&text, QrCodeEcc::Medium) else {
            return;
        };
        let n = code.size();
        let cell = (size.width.min(size.height)) / f64::from(n + 2); // quiet zone
        let off = cell; // one-cell border
        for y in 0..n {
            for x in 0..n {
                if code.get_module(x, y) {
                    frame.fill_rect(
                        kurbo::Rect::new(
                            off + f64::from(x) * cell,
                            off + f64::from(y) * cell,
                            off + f64::from(x + 1) * cell,
                            off + f64::from(y + 1) * cell,
                        ),
                        lumen_render::Brush::Solid(lumen_core::Color::srgb8(20, 22, 30, 0xff)),
                    );
                }
            }
        }
    })
    .id("code");
    el.background = Some(lumen_core::Color::srgb8(255, 255, 255, 0xff));
    el
}

fn build(cx: &mut BuildCx) -> Element {
    let text = cx.signal("text", || "https://lumen.dev".to_string());
    let t = text.get(cx.runtime());
    let mut col = widgets::column(vec![
        widgets::text("QR encoder").id("title"),
        widgets::text_field_basic(cx, "text", &t).id("input"),
        qr_canvas(t),
    ])
    .id("page");
    col.style = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dim::pct(1.0),
        height: Dim::pct(1.0),
        align_items: Some(Align::Center),
        justify_content: Some(Align::Center),
        row_gap: Dim::px(14.0),
        ..LayoutStyle::default()
    };
    col
}
