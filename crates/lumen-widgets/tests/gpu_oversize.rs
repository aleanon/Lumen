//! The reported crash, as a test: a very tall shadowed card on the GPU backend.
//!
//! A downstream app aborted the live window when opening a bulk account. Lumen
//! rasterized a box-shadow as one sprite the size of the element, so a 213-row
//! card at ~12 016 px produced a 12 016 px texture and panicked in
//! `Device::create_texture` labelled 'img'.
//!
//! Nothing here caught it, and the reason is structural rather than an oversight:
//! `DefaultRenderer = TinySkia`, and the CPU renderer has **no
//! texture-dimension concept at all**, so every headless test and golden
//! rendered this scene perfectly while the live window died. A GPU-backed
//! headless mode existed the whole time (`App::with_renderer`) — it was simply
//! never pointed at a large element.
//!
//! These run on a real adapter through the public `App` seam, which is the same
//! construction the shell uses.
#![cfg(feature = "wgpu")]

use lumen_app::element::Shadow;
use lumen_core::codes;
use lumen_core::geometry::Size;
use lumen_layout::Dim;
use lumen_render::gpu::Wgpu;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A shadowed card `h` px tall — the shape that crashed.
fn card(h: f32) -> impl Fn(&mut BuildCx) -> Element {
    move |_cx: &mut BuildCx| {
        let mut el = widgets::column(vec![]);
        el.style.width = Dim::px(200.0);
        el.style.height = Dim::px(h);
        el.background = Some(lumen_core::Color::srgb8(0xff, 0xff, 0xff, 0xff));
        el.corner_radius = 12.0;
        el.shadow = Some(Shadow::soft());
        // Inset so there is a strip to the LEFT of the card that only the
        // shadow can paint. Without it the strip sits under the card's own
        // opaque white fill, and "no ink" would mean "card", not "no shadow".
        let mut root = widgets::column(vec![el.id("card")]);
        root.style.padding = lumen_layout::Edges::all(Dim::px(60.0));
        root.id("root")
    }
}

fn gpu_or_skip() -> Option<Wgpu> {
    match Wgpu::new() {
        Some(g) => Some(g),
        None if std::env::var_os("LUMEN_REQUIRE_GPU").is_some() => {
            panic!("LUMEN_REQUIRE_GPU is set but no wgpu adapter was found")
        }
        None => {
            eprintln!("gpu_oversize: no wgpu adapter; skipping");
            None
        }
    }
}

/// The regression. A 12 016 px card in a 200 px viewport must render.
///
/// The assertions are deliberately stronger than "did not panic": with the
/// shadow 9-slice removed the sprite would be ~12 056 px, which the upload
/// backstop would rescue by downscaling — no crash, but a degraded paint and a
/// `W0110`. Requiring **no** W0110 is what proves the card took the
/// style-sized path rather than the clamp-and-degrade one.
#[test]
fn a_very_tall_shadowed_card_renders_on_the_gpu() {
    let Some(gpu) = gpu_or_skip() else { return };
    let mut h = App::new(card(12_016.0))
        .with_renderer(gpu)
        .run_headless(Size {
            width: 320.0,
            height: 200.0,
        });
    h.pump();
    let img = h.screenshot();
    assert_eq!((img.width(), img.height()), (320, 200));

    let clamped: Vec<_> = h
        .lint()
        .into_iter()
        .filter(|d| d.code == codes::W0110)
        .collect();
    assert!(
        clamped.is_empty(),
        "a tall card must rasterize a style-sized shadow, not be rescued by the \
         upload clamp: {clamped:?}"
    );
}

/// The same card at an ordinary height, to show the tall case is not passing
/// because the shadow silently stopped being drawn.
#[test]
fn the_tall_card_paints_the_same_shadow_as_a_short_one() {
    let Some(_) = gpu_or_skip() else { return };
    let ink = |h: f32| {
        let gpu = Wgpu::new().expect("adapter checked above");
        let mut app = App::new(card(h)).with_renderer(gpu).run_headless(Size {
            width: 320.0,
            height: 200.0,
        });
        app.pump();
        let img = app.screenshot();
        // The card starts at x=60; sample x 30..55, which only its shadow can
        // reach.
        let p = img.pixels();
        (0..200u32)
            .flat_map(|y| (30..55u32).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let i = ((y * 320 + x) * 4) as usize;
                p[i] < 250 || p[i + 1] < 250 || p[i + 2] < 250
            })
            .count()
    };
    let short = ink(150.0);
    let tall = ink(12_016.0);
    assert!(
        short > 0,
        "the reference card should cast a visible shadow to its left"
    );
    assert!(
        tall >= short,
        "the tall card casts {tall} shadow px against the short card's {short}; \
         a tall card covers more of that strip, so fewer means the shadow \
         stopped being drawn rather than being sliced"
    );
}
