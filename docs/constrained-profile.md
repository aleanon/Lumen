# The constrained profile (CFG1) — what it reaches today, and what blocks the rest

*2026-08-08. Measured, not projected.*

The owner's A+ resource bar asks for internals swappable enough to "meet every
use case" — a build targeting heavily resource-constrained equipment, while the
default stays full-power. This records how far that is actually reachable now,
with numbers, and names the one thing standing in the way of the rest.

## The measured span

All figures: release, `lto = true`, `strip = true`, `opt-level = "z"`, x86-64
Linux, a counter app with a button. Built **outside the workspace**, because
Cargo unifies features across workspace members and would otherwise hide the
lean result entirely.

| profile | binary | shared libs |
|---|---:|---:|
| default (pan-Unicode face, snapshot, desktop-integration, wgpu) | 22.0 MB | 70 |
| lean **windowed** — `default-features = false, features = ["wgpu"]` | **13.3 MB** | 5 |
| lean **headless** — same features, no `lumen::run` | 6.8 MB | 5 |

At `opt-level = 3` the same lean windowed build is 18.3 MB and the full default
33.8 MB, so the numbers above are the `opt-z` shape, which is what a constrained
target would use.

What the lean profile drops, and what each is worth:

- **`pan-unicode`** — the 15 MB embedded pan-Unicode face becomes the ~355 KB
  Latin+symbols subset. The single largest lever by a wide margin.
- **`snapshot`** — drops `serde_json` and the `Serialize` bound on state.
- **`desktop-integration`** (GX2) — drops `rfd` + `muda` + `tray-icon` + `gtk`.
  Only ~0.33 MB of binary, because GTK is dynamically linked, but it takes the
  shared-library count from **70 to 5**. On a constrained target that is the
  difference between needing a GTK stack present and not.
- **`LUMEN_A11Y=0`** (GX4, runtime not compile-time) — drops the AccessKit
  D-Bus thread: measured 13 threads / 12 socket fds down to 11 / 10 on a live
  window.

## The blocker for the rest: there is no CPU presentation path

`01 §9` budgets a hello-world binary at **<5 MB**. The lean *headless* build is
6.8 MB and the lean *windowed* build is 13.3 MB, and the gap between them is
almost entirely **wgpu**.

This is not a feature-gating problem. `lumen-shell` depends on `wgpu`
unconditionally (55 use sites), and `lumen-shell`'s own `wgpu` feature is
**forwarding-only** — it enables `lumen-widgets/wgpu` and does nothing about the
shell's own dependency. The reason it cannot simply be gated is structural:

> `Presenter` (`lumen-shell/src/lib.rs`) blits the **CPU-rendered** frame to the
> window through a wgpu surface. The desktop shell has exactly one presentation
> path, and it runs on the GPU even when the renderer of record is the CPU one
> (ADR-002).

So "no-GPU, software-render" — the plan's own words for CFG1 — is **new work,
not configuration**: it needs a second presentation backend (softbuffer, or
direct X11/Wayland blitting) before wgpu can become optional. Until that exists,
<5 MB is unreachable on desktop no matter which features are turned off, and the
budget should be read as aspirational rather than missed.

Mobile and web are unaffected by this particular blocker — they have their own
shells (`lumen-shell-android` / `-ios` / `-web`), and the iOS shell already
presents through CoreGraphics rather than Metal.

## The gate was measuring the wrong binary

`scripts/size_gate.sh`'s lean leg called `run_headless`, so LTO dropped the
entire presentation path as dead code. Same features, same profile, built back
to back: **6.8 MB headless vs 13.3 MB windowed.** The gate was reporting about
half the shipped size, and `01 §9`'s "hello-world binary" — a budget that
plainly means a GUI app with a window — was being checked against a binary with
no window in it.

A third leg now builds a windowed app (build only; it never opens a window, so
it stays headless-CI safe) with a 16 MB regression guard against today's 13.3.
That guard is **not** a target; the target is still <5 MB and needs the CPU
presentation path above.

This is the same defect class as the GPU parity suite that self-skipped when no
adapter was present, and the ABI hash that fingerprinted nothing: green, and
asserting less than it appears to.

## Building it

```sh
# Compile-time: drop the font, the snapshot surface, and the GTK cluster.
cargo build --release --no-default-features --features wgpu
```

```sh
# Runtime: drop the AccessKit D-Bus thread.
LUMEN_A11Y=0 ./my-app          # or the GTK/Qt-standard NO_AT_BRIDGE=1
```

`wgpu` stays in that feature list because, per the section above, it currently
has to.

## What is left for a complete CFG1

1. **A CPU presentation backend**, so `wgpu` can become optional in
   `lumen-shell`. This is the gating item — everything below is smaller.
2. **`PlatformConfig` (MOD1)** so the profile is selected through one bundle
   type rather than a hand-assembled feature list. The plan scoped CFG1 as
   "selected through `PlatformConfig`"; what exists today is the feature set,
   not the selector.
3. **Rename or split `lumen-shell`'s `wgpu` feature.** It forwards to
   `lumen-widgets` and does not gate the shell's own `wgpu` dependency, so its
   name currently promises something it does not do.
