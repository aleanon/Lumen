//! qr — M.6: live QR encoding (pure-Rust `qrcodegen`) drawn through the
//! immediate-mode Canvas: type text, the code re-encodes and repaints.
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element, Stack};
use qrcodegen::{QrCode, QrCodeEcc};

/// Build the QR app.
pub fn main_app() -> App {
    App::view(build)
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

fn build(cx: &mut BuildCx) -> impl lumen_widgets::Direct {
    // E2b: statement form. The keyed `text` signal stays keyed — the field
    // widget owns it (D1) — so it is read here and its widget built eagerly,
    // then moved into the body.
    let text = cx.signal("text", || "https://lumen.dev".to_string());
    let t = text.get(cx.runtime());
    let field = widgets::text_field_basic(cx, "text", &t).id("input");
    let code = qr_canvas(t);
    Stack::column(move |c| {
        c.child(widgets::text("QR encoder").id("title"));
        c.child(field);
        c.child(code);
    })
    .width(Dim::pct(1.0))
    .height(Dim::pct(1.0))
    .centered()
    .gap(14.0)
    .id("page")
}
