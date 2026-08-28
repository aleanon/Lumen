//! W0110: an element whose shadow sprite exceeds the portable texture limit.
//!
//! The advisory is checked against a FIXED 2048 px — the WebGL2/downlevel floor
//! — on whatever backend is running, including the CPU one. That is the point:
//! the shadow that crashed a live GPU window rendered perfectly in every
//! headless test, so a limit-aware check that only runs on the GPU would have
//! stayed just as blind.

use kurbo::Size;
use lumen_app::element::Shadow;
use lumen_core::codes;
use lumen_layout::Dim;
use lumen_widgets::{widgets, App, BuildCx, Element};

fn shadowed(blur: f64, w: f32, h: f32) -> impl Fn(&mut BuildCx) -> Element {
    move |_cx: &mut BuildCx| {
        let mut el = widgets::column(vec![]);
        el.style.width = Dim::px(w);
        el.style.height = Dim::px(h);
        el.background = Some(lumen_core::Color::srgb8(0xff, 0xff, 0xff, 0xff));
        el.corner_radius = 12.0;
        el.rare_mut().shadow = Some(Shadow {
            dx: 0.0,
            dy: 2.0,
            blur,
            spread: 0.0,
            color: lumen_core::Color::srgb8(0, 0, 0, 60),
        });
        widgets::column(vec![el.id("card")]).id("root")
    }
}

/// A tall card is NOT a finding: the 9-slice bounds its sprite by style, so
/// height no longer drives sprite size. Reporting it would be the lint crying
/// about the case that was just fixed.
#[test]
fn a_tall_card_with_an_ordinary_shadow_is_not_flagged() {
    let mut h = App::new(shadowed(18.0, 300.0, 12_000.0)).run_headless(Size::new(400.0, 800.0));
    h.pump();
    assert!(
        !h.lint().iter().any(|d| d.code == codes::W0110),
        "a 12 000 px card with an 18 px blur rasterizes a style-sized sprite; \
         flagging it would report the bug that was fixed"
    );
}

/// An enormous blur IS a finding — that is what actually drives sprite size now.
///
/// Note how large it has to be. The sprite is
/// `min(len, 2*(radius + 3*blur) + 1) + 2*(blur + 2)`, so for a 300 px box a
/// 700 px blur still only needs 1704 px and is correctly NOT reported. It takes
/// 900 px of blur to pass 2048. That the advisory is now this hard to trip is
/// the 9-slice working, not the check being broken.
#[test]
fn an_enormous_blur_is_flagged_on_the_cpu_backend() {
    let mut h = App::new(shadowed(900.0, 300.0, 300.0)).run_headless(Size::new(400.0, 400.0));
    h.pump();
    let found: Vec<_> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == codes::W0110)
        .collect();
    assert!(
        !found.is_empty(),
        "a 700 px blur needs a sprite past the 2048 px portable limit and must \
         be reported, on the CPU backend too"
    );
    assert!(
        found[0].message.contains("#card"),
        "the finding must name the element: {}",
        found[0].message
    );
}
