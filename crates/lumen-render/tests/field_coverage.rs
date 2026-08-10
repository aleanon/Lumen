//! Does each backend actually CONSUME every display-list field?
//!
//! The existing parity suite compares backends on a corpus of *scenes*. That
//! catches a backend rendering a scene wrongly — but not a backend silently
//! ignoring a field no scene happens to use. Those two are different questions,
//! and only the first had a test.
//!
//! It cost a shipped bug to notice. `DrawCmd::PushLayer` carries a
//! `transform: Affine`; the CPU backend honours it, `gpu.rs` never reads it. A
//! `.lss` `transform` property was implemented against that field, passed every
//! test (they render on CPU, which is also what the desktop shell uses), and
//! would have been a silent no-op for any `--wgpu` app. It was reverted.
//!
//! # The method
//!
//! For each field, build two display lists differing in **that field alone** and
//! assert each backend renders them DIFFERENTLY. If a backend ignores the field,
//! the two renders are byte-identical and the test says which field and which
//! backend.
//!
//! This deliberately does not compare CPU against GPU — that comparison is the
//! other suite's job and is complicated by colour space. Here each backend is
//! only ever compared with itself, so the assertion is exact and the failure
//! message is unambiguous.
//!
//! # Known gaps are asserted, not skipped
//!
//! # The case list is hand-written, and that is this file's weakness
//!
//! This test claims each backend consumes *every* display-list field, but it
//! only checks the fields someone enumerated in `cases()`. `Image::src_rect`
//! was absent from that list for as long as the file existed, and the GPU
//! backend was silently dropping it the whole time (`DrawCmd::Image { .., .. }`
//! destructured it away) — so every partial-region blit, i.e. every box-shadow
//! band, painted the whole sprite squashed into the band. A hand-written list
//! proves what was remembered, not what exists. **When you add a field to
//! `DrawCmd`, add a case here in the same commit.**
//!
//! `GPU_IGNORES` lists fields the GPU backend does not consume today. They are
//! asserted to STILL be ignored, so the list is a tripwire in both directions:
//! implementing one fails this test until the entry is removed, and a new gap
//! fails it immediately. A skip would let the set grow silently, which is the
//! defect class this file exists to close.
#![cfg(feature = "wgpu")]

mod common;

use common::*;
use kurbo::{Affine, Rect};
use lumen_core::Color;
use lumen_render::display_list::*;
use lumen_render::gpu::Wgpu;
use lumen_render::{cpu, RgbaImage};

/// Fields `gpu.rs` does not read today. Each entry needs a reason and a cost.
///
/// Empty as of 2026-08-08: `PushLayer::transform` was the only entry, and the
/// composite shader now applies the affine (the linear part rides in a new
/// vertex attribute, the translation in `params`' two spare slots).
const GPU_IGNORES: &[&str] = &[];

fn size() -> (u32, u32) {
    (200, 140)
}

/// Two lists differing only in the named field.
struct FieldCase {
    field: &'static str,
    a: DisplayList,
    b: DisplayList,
}

fn base_rect(dl: &mut DisplayList) {
    dl.push(DrawCmd::Rect {
        rect: Rect::new(40.0, 30.0, 160.0, 110.0),
        brush: Brush::Solid(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
        radii: CornerRadii::all(0.0),
        border: None,
    });
}

/// A layer wrapping one opaque rect, parameterised by the field under test.
fn layered(
    clip: Option<RoundedRect>,
    opacity: f32,
    transform: Affine,
    blend: BlendMode,
) -> DisplayList {
    layered_blur(clip, opacity, transform, blend, 0.0)
}

fn layered_blur(
    clip: Option<RoundedRect>,
    opacity: f32,
    transform: Affine,
    blend: BlendMode,
    filter_blur: f32,
) -> DisplayList {
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::PushLayer {
        clip,
        opacity,
        transform,
        filter_blur,
        blend,
    });
    base_rect(&mut dl);
    dl.push(DrawCmd::PopLayer);
    dl
}

fn cases() -> Vec<FieldCase> {
    let plain = || layered(None, 1.0, Affine::IDENTITY, BlendMode::SourceOver);
    vec![
        FieldCase {
            field: "PushLayer::transform",
            a: plain(),
            // A large translate: whatever the compositing model, moving the
            // layer 40px must change the frame.
            b: layered(
                None,
                1.0,
                Affine::translate((40.0, 20.0)),
                BlendMode::SourceOver,
            ),
        },
        FieldCase {
            field: "PushLayer::filter_blur",
            a: plain(),
            b: layered_blur(None, 1.0, Affine::IDENTITY, BlendMode::SourceOver, 6.0),
        },
        FieldCase {
            field: "PushLayer::opacity",
            a: plain(),
            b: layered(None, 0.35, Affine::IDENTITY, BlendMode::SourceOver),
        },
        FieldCase {
            field: "PushLayer::clip",
            a: plain(),
            b: layered(
                Some(RoundedRect {
                    rect: Rect::new(60.0, 40.0, 120.0, 90.0),
                    radii: CornerRadii::all(0.0),
                }),
                1.0,
                Affine::IDENTITY,
                BlendMode::SourceOver,
            ),
        },
        FieldCase {
            field: "Image::src_rect",
            // A 2x1 image, left half red and right half blue, blitted into the
            // same destination from each half. A backend that ignores
            // `src_rect` samples the whole texture both times and renders them
            // identically — which is exactly what the GPU did.
            a: image_src(Rect::new(0.0, 0.0, 1.0, 1.0)),
            b: image_src(Rect::new(1.0, 0.0, 2.0, 1.0)),
        },
        FieldCase {
            field: "Rect::radii",
            a: {
                let mut dl = DisplayList::new();
                base_rect(&mut dl);
                dl
            },
            b: {
                let mut dl = DisplayList::new();
                dl.push(DrawCmd::Rect {
                    rect: Rect::new(40.0, 30.0, 160.0, 110.0),
                    brush: Brush::Solid(Color::srgb8(0x1a, 0x73, 0xe8, 0xff)),
                    radii: CornerRadii::all(24.0),
                    border: None,
                });
                dl
            },
        },
    ]
}

/// A 2x1 red|blue image drawn into the frame, sampling `src`.
fn image_src(src: Rect) -> DisplayList {
    let mut dl = DisplayList::new();
    let mut px = Vec::new();
    px.extend_from_slice(&Color::srgb8(0xd0, 0x20, 0x20, 0xff).to_srgb8());
    px.extend_from_slice(&Color::srgb8(0x20, 0x20, 0xd0, 0xff).to_srgb8());
    dl.images.push(RgbaImage::from_raw(2, 1, px));
    dl.push(DrawCmd::Image {
        id: ImageId(0),
        src_rect: src,
        dst_rect: Rect::new(40.0, 30.0, 160.0, 110.0),
        quality: Filter::Nearest,
    });
    dl
}

fn cpu_render(dl: &DisplayList) -> RgbaImage {
    let (w, h) = size();
    cpu::render(dl, w, h, bg())
}

fn gpu_render(gpu: &Wgpu, dl: &DisplayList) -> RgbaImage {
    let (w, h) = size();
    gpu.render(dl, w, h, bg())
}

#[test]
fn cpu_consumes_every_display_list_field() {
    for c in cases() {
        let a = cpu_render(&c.a);
        let b = cpu_render(&c.b);
        assert_ne!(
            a.pixels(),
            b.pixels(),
            "the CPU backend ignores `{}`: two lists differing only in that \
             field rendered byte-identically",
            c.field
        );
    }
}

#[test]
fn gpu_consumes_every_field_except_the_known_gaps() {
    let Some(gpu) = require_gpu_or_skip() else {
        return;
    };
    for c in cases() {
        let a = gpu_render(&gpu, &c.a);
        let b = gpu_render(&gpu, &c.b);
        let consumed = a.pixels() != b.pixels();
        let expected_gap = GPU_IGNORES.contains(&c.field);
        if expected_gap {
            assert!(
                !consumed,
                "`{}` is listed in GPU_IGNORES but the GPU now CONSUMES it — \
                 implement-and-forget is the failure this list prevents. Remove \
                 the entry (and the note in 04 §8).",
                c.field
            );
        } else {
            assert!(
                consumed,
                "the GPU backend ignores `{}`: two lists differing only in that \
                 field rendered byte-identically. Either implement it, or add it \
                 to GPU_IGNORES with a reason and a cost.",
                c.field
            );
        }
    }
}

/// Same skip policy as the parity suite: harmless without an adapter, but a
/// failure under `LUMEN_REQUIRE_GPU` so a GPU job that lost its driver reports
/// broken rather than green.
fn require_gpu_or_skip() -> Option<Wgpu> {
    match Wgpu::new() {
        Some(g) => Some(g),
        None if std::env::var_os("LUMEN_REQUIRE_GPU").is_some() => {
            panic!("LUMEN_REQUIRE_GPU is set but no wgpu adapter was found")
        }
        None => {
            eprintln!("skipping: no wgpu adapter");
            None
        }
    }
}
