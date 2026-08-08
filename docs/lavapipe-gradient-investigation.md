# Gradients render blank under lavapipe — investigation

*2026-08-08. Narrowed, not fixed. Recorded because six plausible causes are now
eliminated, and re-deriving that costs more than reading it.*

This blocks making the `gpu` CI job a required check, which in turn blocks R6
(see `docs/r6-gpu-damage-scope.md`): GPU regressions are invisible to the CPU
goldens by construction (ADR-002), so R6 cannot land safely without a working
GPU gate.

## The symptom, precisely

Under lavapipe (Mesa software Vulkan), **every** gradient scene renders a frame
byte-identical to the blank reference. On this box's NVIDIA adapter the same
scenes render correctly.

| scene | NVIDIA | lavapipe |
|---|---:|---:|
| `rect_solid` | 16 800 px | **16 800 px** |
| `gradient_linear` | 23 400 px | **0** |
| `gradient_radial` | 23 400 px | **0** |
| `gradient_conic` | 23 400 px | **0** |
| `gradient_rounded` | 17 640 px | **0** |

(px = pixels differing from a blank frame.) Non-gradient scenes are unaffected,
including `image_checker`, which matters — see below.

Reproduce:

```sh
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
  LUMEN_REQUIRE_GPU=1 cargo test -p lumen-render --test cpu_vs_gpu --features wgpu
```

## Localised to one operation

Replacing the ramp lookup in `gradient_fs` with a constant colour makes the
gradient quads render **exactly** as on NVIDIA (23 400 px). So the instance
data, the vertex stage, the pipeline, the blend state and the draw call are all
correct.

> **The failure is `textureSample` of the ramp texture returning zeros.**

Zeros mean alpha 0, which alpha-blends to nothing — matching "identical to
blank" exactly.

## Hypotheses eliminated

Each was tested by modifying the code and re-running the probe:

| # | Hypothesis | Test | Result |
|---|---|---|---|
| 1 | 1-pixel-tall texture is mishandled | ramp built 512×2, rows duplicated | still blank |
| 2 | Linear filtering unsupported on this format | `ramp_sampler` → `Nearest` | still blank |
| 3 | `write_texture` not flushed before the pass | `queue.submit(empty())` after the write | still blank |
| 4 | Texture/view freed before the pass executes | pushed both into `KeepAlive` | still blank |
| 5 | Sample coordinate is NaN or out of range | sampled a **literal** `(0.5, 0.5)` | still blank |
| 6 | Bind group index or shader binding mismatch | read both: group 1, bindings 0/1 — identical to the image path | not the cause |

No wgpu validation errors are emitted at any point (`RUST_LOG=wgpu_core=warn`).

## The awkward part

`upload_image` and `upload_ramp` are, line for line, the **same** operation:

- same `TARGET_FORMAT` (`Rgba8UnormSrgb`)
- same usage (`TEXTURE_BINDING | COPY_DST`)
- same `mip_level_count: 1`, `sample_count: 1`, `D2`
- same `queue.write_texture` shape
- same `image_bgl` layout (`Float { filterable: true }` + `Filtering`)
- same bind group indices and shader declarations

and after hypothesis 2 the samplers are field-identical too. The image path
works under lavapipe; the ramp path does not. The only remaining difference is
the texture's **dimensions** (512×1, or 512×2 after hypothesis 1) versus an
image's.

## Where that leaves it

The elimination points at a driver-level defect in lavapipe for this texture
configuration, rather than a bug in Lumen. That is a *conclusion by exhaustion*,
not a proof, and it is worth saying which is which.

Next steps, cheapest first:

1. **Try a larger ramp** — e.g. 512×64. If it renders, the bound is a minimum
   dimension and the workaround is trivial (and would be worth taking purely to
   unblock GPU CI, even as a driver workaround).
2. **Read the texels back** with `copy_texture_to_buffer` immediately after the
   write. That distinguishes "the write never landed" from "the sample fails",
   which is the one question the probes above cannot separate.
3. **Try `Rgba8Unorm`** (non-sRGB) for the ramp. sRGB sampling is the one format
   behaviour not yet isolated.
4. If it is lavapipe, file upstream and pin the workaround with a comment
   pointing here.

## What must not happen

Do not make the `gpu` job required by skipping the gradient scenes. The
allow-list already carries the scenes that must be exactly correct; quietly
removing the four that fail would leave a green job asserting less than it
appears to — the same defect class as the ABI hash that fingerprinted nothing
(HR1) and the parity suite that self-skipped when no adapter was present (GX1).
