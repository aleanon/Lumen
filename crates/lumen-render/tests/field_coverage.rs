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
/// * `PushLayer::transform` — the composite path emits a `CompositeInstance`
///   with `rect`/`radii`/`params` and blits the child texture axis-aligned.
///   Honouring the affine means growing that struct (its `params` has only two
///   spare floats) and applying the matrix in the composite shader's vertex
///   stage. Bounded, not started.
const GPU_IGNORES: &[&str] = &["PushLayer::transform"];

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
    let mut dl = DisplayList::new();
    dl.push(DrawCmd::PushLayer {
        clip,
        opacity,
        transform,
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
