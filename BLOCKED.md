# BLOCKED — the CPU presentation backend needs an ADR-003 decision

*Raised 2026-08-08. Per `07 §2`: stop the affected task, write options +
recommendation, continue elsewhere. This is a decision for the owner, not for
whoever hits it.*

## What is blocked

A **CPU presentation path** for the desktop shell. It is the gating item for
three separate things, which is why it is worth deciding rather than deferring:

* **`01 §9`'s `<5 MB` hello-world budget.** Unreachable today by any feature
  combination — measured 13.3 MB for a lean *windowed* build against 6.8 MB
  headless, and the gap is almost entirely wgpu.
* **CFG1's constrained profile.** The plan specifies "no-GPU, software-render";
  that is currently not configurable, it is absent.
* **Making `lumen-shell`'s `wgpu` feature honest.** It is forwarding-only today
  (it enables `lumen-widgets/wgpu` and does nothing about the shell's own
  unconditional `wgpu` dependency, 55 use sites).

## Why it cannot just be feature-gated

`Presenter` (`lumen-shell/src/lib.rs`) blits the **CPU-rendered** frame to the
window through a wgpu surface. The desktop shell has exactly one presentation
path and it runs on the GPU — even though the CPU renderer is the renderer of
record (ADR-002). Turning wgpu off does not select a different path; it removes
the only one. This is missing capability, not missing configuration.

## Why it is an escalation

Presenting a pixel buffer to a native window requires platform surface code that
`winit` deliberately does not provide. The obvious crate is **`softbuffer`**, and
it is **not in the ADR-003 whitelist**, which `07 §2` lists as an escalation:
*"Any new runtime dependency outside ADR-003."*

## Option A — add `softbuffer` (recommended)

Measured, not estimated. `cargo tree` on a probe crate against `lumen-shell`'s
existing tree:

* softbuffer's full tree is **52 crates**, of which **45 are already in
  `lumen-shell`** via winit.
* **Marginal cost: 7 crates** — `softbuffer`, `tiny-xlib`, `drm`, `drm-ffi`,
  `drm-fourcc`, `drm-sys`, `ctor`.

Against: it is a seventh platform-surface dependency and pulls a small DRM stack
on Linux. For: it is maintained by the Rust windowing group (the same
organisation as winit), is the de-facto answer to exactly this problem, and 45 of
its 52 crates are already paid for.

**Recommended** because the alternative is to write and maintain the same code
for X11, Wayland, Win32 and AppKit ourselves, which is strictly more surface area
for strictly less review.

## Option B — hand-rolled platform presentation

Write the blit per platform against the raw window handle. No new dependency,
and full control of the code path.

Against: it is four platform backends of unsafe FFI, it duplicates a maintained
crate, and it puts Lumen in the business of window-system surface management,
which is not the project's competence or interest. Realistically weeks, and the
maintenance never ends.

## Option C — decline, and restate the budget

Accept that the desktop shell requires a GPU, drop `<5 MB` as a desktop target,
and scope CFG1's constrained profile to **mobile and web only** — both of which
have their own shells and are unaffected by this blocker (the iOS shell already
presents through CoreGraphics rather than Metal).

This is a coherent position, not a cop-out: every mainstream desktop toolkit
assumes a compositor. It should be chosen deliberately rather than arrived at by
never deciding, which is the status quo.

## What was done instead

Everything downstream of this decision that does not depend on it:

* `docs/constrained-profile.md` — the measured profile span, what each dropped
  feature is worth, and this blocker named as the gating item.
* `scripts/size_gate.sh` — a **windowed** leg, because the existing lean leg
  called `run_headless` and LTO dropped the whole presentation path, so the gate
  was reporting 6.8 MB for a configuration that ships at 13.3 MB.
* `01 §9` — the binary budget restated as a measured span across five
  configurations, with the structural reason `<5 MB` is not currently reachable.
