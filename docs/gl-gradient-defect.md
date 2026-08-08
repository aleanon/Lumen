# The gradient bug was never lavapipe's. It was ours, on OpenGL.

*2026-08-08.*

R6 (damage into the GPU present path) was filed as **ungateable**, on this
reasoning (`docs/r6-gpu-damage-scope.md`):

> The CPU goldens cannot see GPU regressions by construction (ADR-002), and the
> lavapipe job that would catch them **fails on gradients** — so R6 would land
> with no automated way to prove it did not break rendering.

That is now fixed, and the diagnosis in it was wrong in a way worth recording.

## What was actually happening

`Wgpu::new` requested an adapter over `Backends::all()` with
`PowerPreference::HighPerformance`. On a machine with both an NVIDIA Vulkan ICD
and NVIDIA OpenGL, **wgpu answered with the GL adapter** — and on the GL path
`textureSample` of the 512×1 Oklab gradient ramp returns zeros. Alpha zero,
nothing drawn: **every gradient in the frame silently disappears**, with no
validation error and no warning.

So this was a real, shipped, user-facing defect on any machine where wgpu
resolves to GL — not merely a CI annoyance.

## How it got attributed to lavapipe

`VK_DRIVER_FILES` constrains **Vulkan only**. Pointing it at the lavapipe ICD
and re-running does not force a software renderer; it removes the NVIDIA
*Vulkan* adapter and leaves NVIDIA *GL* to win the `HighPerformance` selection.
The suite then fails, on the run where lavapipe was "switched on" — and the
failure gets pinned on lavapipe.

I made exactly this mistake here, and got four probe results deep into "a
lavapipe defect" before the adapter name printed
`NVIDIA GeForce RTX 4070/PCIe/SSE2` — a GL renderer string — and contradicted
the label I had put on the run. **The suite never printed which adapter it was
using, so there was nothing to check the assumption against.** It prints it now.

On genuine lavapipe (Vulkan-only instance), `gradient_linear` renders correctly
and matches the CPU **better than any other scene in the corpus** — ΔE 0.0039,
against 0.16 for rounded rects and 0.30 for paths.

## The fix

`Wgpu::new` now tries `Backends::PRIMARY` (Vulkan/Metal/DX12) and falls back to
`SECONDARY` only if nothing there answers. GL stays reachable for hardware that
has nothing else; it is no longer preferred over a working Vulkan driver.

Four Lumen-side explanations for the GL failure were tested and falsified before
concluding it is GL's sampling path:

| hypothesis | test | result |
|---|---|---|
| our geometry/instance data | `textureLoad` instead of `textureSample` | renders correctly — same pipeline, same bind group |
| implicit-derivative LOD | `textureSampleLevel(..., 0.0)` | still blank |
| sRGB filtering unsupported | the bilinear *image* scene, same backend | filters an sRGB texture fine |
| one-texel-tall texture | two-row ramp | fails identically |
| texture dropped after bind-group creation | `mem::forget` the handle | no change |

## What this unblocks

**R6 is gateable.** The full 16-suite render set passes on lavapipe under
`LUMEN_REQUIRE_GPU=1` — including `cpu_vs_gpu`, `field_coverage`, `backdrop`,
`gpu_glyph_run` and `damage_equivalence`. The stated precondition for starting
R6 ("fix the lavapipe gradient failure → broaden GPU parity") is met, and it was
met by a five-line change to adapter selection rather than by the driver work
the estimate assumed.

`GL_GAPS` in `tests/cpu_vs_gpu.rs` keeps the GL defect asserted rather than
skipped, keyed on the reported *backend* rather than a vendor string. When GL is
fixed, the test fails and says to delete the entry.

## The transferable part

A test that self-selects a resource should say which one it selected. This suite
had a `LUMEN_REQUIRE_GPU` escape hatch precisely because "silently did nothing"
is a known failure mode for it — but it had no equivalent guard against
"silently did it somewhere else", which is how a defect on the default desktop
path spent months recorded as a software-rasteriser quirk that only CI would
ever hit.
