# Path to A+: resource usage (desktop + mobile)

*Research note, 2026-08-07. Companion to `.ai_docs/review-2026-08/04-resource-usage.md`
(C+ desktop / D mobile), `00-SYNTHESIS.md`, `.ai_docs/01-architecture.md` §9,
and `/home/aleksander/.claude/plans/zippy-dancing-allen.md` (the approved
campaign, rev 2 — predicts **B+ desktop / C- mobile at best**, and says so
itself: *"This campaign does not reach A+ anywhere"*). Every claim below is
tagged **[MEASURED]** (I ran the command/read the byte count myself this
session), **[DOCUMENTED]** (a number from the project's own docs/scripts,
not reproduced by me), or **[ESTIMATE/INFERENCE]** (reasoning without a
number behind it). No `cargo build --workspace` or clean release build was
run — disk was 76 GB free with `target/` at 70-71 GB. See the *Measured vs
estimated* table at the end for the complete audit trail.*

---

## Verdict

**Desktop: A+ is reachable, but not on the campaign's current scope, and not
without spending three things the campaign explicitly declines or defers: a
softbuffer CPU-present path, a GTK-cluster feature gate, and a restated
binary-size target.** None of the desktop blockers found in this
investigation are architectural rewrites — every one is bounded, scoped, and
either already has a proven pattern elsewhere in the codebase (cache capping,
target-gating) or a written-but-unexecuted design (softbuffer). The campaign
predicts B+ because it explicitly caps its own ambition at the mechanical
fixes already scoped in M-E and stops short of the GPU-context work and the
GTK-cluster decision. Doing both is 4-8 more weeks past the campaign, not a
different architecture.

**Mobile: A+ is reachable for the *idle/static* case almost immediately (the
CPU-render choice is directionally correct for battery, matching Slint's own
software-renderer thesis) but is genuinely unproven for the *animated/scroll*
case, because nobody has measured whether whole-screen CPU rasterization at
real phone resolutions can hit the framework's own 60 fps mid-range-mobile
target.** This is the decisive finding of this report: mobile's D grade is
**mostly unfinished work** (memory-pressure wiring, cache clearing, CI, a
compiled-in-but-unused `wgpu` dependency that should simply be feature-gated
off) plus **one open architectural question that is gated on a measurement
that has never been taken** (CP4 in the campaign). It is not a structural dead
end — but it is not proven sound, either, and "proven sound" requires an ARM
device benchmark that doesn't exist yet.

**Both verdicts are conditional on accepting a restated `<5 MB` target.**
The spec's own number is not reachable for a GPU-accelerated, GTK-integrated
desktop build; it *is* reachable for a CPU-only, no-native-dialog build,
which is closer to what Slint's software renderer ships than to what "hello
world, full native chrome" typically means. See the binary-size section for
the specific floor this session measured.

---

## 1. The binary-size analysis

### 1.1 What the binary is actually made of — measured, not estimated

**[MEASURED]** `target/release/hello` (default features, stripped, release):
23,151,024 bytes. `size -A` on that exact artifact:

| Section | Bytes |
|---|---|
| `.rodata` | 19,853,448 |
| `.text` | 2,823,398 |
| `.eh_frame` | 163,948 |
| `.data.rel.ro` | 99,008 |

**[MEASURED]** Bundled fonts, `ls -la crates/lumen-text/fonts/`:
`GoNotoKurrent-Regular.ttf` = 15,515,760 B (the default CJK/RTL/Indic face),
`GoNotoKurrent-Latin.ttf` = 354,748 B (the lean subset), `DejaVuSans-Symbols.ttf`
= 171,732 B (always embedded, both profiles).

Combining these two measurements (default face + symbols face, both
`include_bytes!`'d unconditionally in the default profile):

- **Font bytes = 67.8% of the entire stripped binary, 79.0% of `.rodata`.**
- **Non-font floor = 23,151,024 − 15,687,492 = 7,463,532 bytes (≈7.46 MB).**

This last number is the headline finding for this section. It is **larger
than the spec's entire `<5 MB` target**, before a single byte of font is
counted. Cross-check: the project's own documented lean-profile figure
**[DOCUMENTED, not reproduced this session]** is 7.5 MB
(`scripts/size_gate.sh:6`), and the lean profile embeds the 527 KB
Latin+symbols pair, not zero bytes — so its own non-font floor is
`7.5 − 0.527 ≈ 6.97 MB`. Two independently-obtained numbers (one from
disassembling the *default* binary's section table, one from the project's
own *lean*-profile CI gate) converge on **a ~7 MB non-font floor**. That
convergence is the strongest evidence in this report that font policy alone
cannot reach `<5 MB`.

### 1.2 Where the ~7 MB floor comes from

**[MEASURED]** `cargo tree -p wgpu --offline -e normal` (host target,
x86_64-linux) resolves `ash` (Vulkan) + `glow`/`khronos-egl` (GLES) under
`wgpu-hal` — 63 unique packages by `--prefix none | sort -u | wc -l` (a prior
review's independent count via a different tree-flattening method got 97,
which double-counts shared deps appearing on multiple branches; both are
legitimate, differently-scoped counts of the same subtree). **Non-Linux HAL
backends are already excluded**: re-running with `--target aarch64-apple-darwin`
pulls in `metal`/`core-graphics-types`/`block`, and `--target x86_64-pc-windows-msvc`
pulls in the `windows-*`/`d3d12` chain — neither appears in the default
Linux-host resolution. **This answers the task's question directly: wgpu's
backends are already target-gated by Cargo's own per-target dependency
resolution; there is no further "only ship one backend" lever to pull that
isn't already pulled.** The only remaining wgpu-side levers are (a) restrict
`wgpu-hal`'s own feature list to a single backend (Vulkan-only, dropping
`glow`/`khronos-egl` on Linux) via `default-features = false` — not done
today, saving unmeasured — or (b) drop wgpu entirely for a CPU-only build.

**[READ FROM SOURCE]** `crates/lumen-render/Cargo.toml`, `crates/lumen-widgets/Cargo.toml`,
`crates/lumen/Cargo.toml`: a wgpu-less, CPU-only (tiny-skia) build path
**already exists and is wired** (`--no-default-features` on `lumen-render`/
`lumen-widgets` drops the whole wgpu/lyon/ash/glow/naga/wgpu-core/wgpu-hal
subtree). It was never exercised for a size measurement in this session
(would require a fresh build, disallowed) — but it is not a proposal, it is
dead-simple to try.

**[READ FROM SOURCE — important correction to the codebase's own framing]**
`Cargo.toml:126-127`'s comment claims `complex-scripts` is needed "or parley
panics ('no segmentation model for language: ja')." Reading parley 0.11.0's
actual source (`~/.cargo/registry/src/…/parley-0.11.0/src/analysis/mod.rs`)
shows this is **false for the currently-pinned version**: without
`complex-scripts`, parley calls `WordSegmenter::new_for_non_complex_scripts`
instead of `new_dictionary` — a graceful degradation to rule-based
segmentation, not a panic. (A repo-wide grep of parley 0.11.0's source for
`panic!`/`unimplemented!`/`todo!` finds none in the segmentation path; the
claim matches parley 0.7.0-era behavior, which had no `complex-scripts`
feature at all — a stale doc comment, flaggable under `AGENT.md`'s
doc-currency rule.) **More consequentially: parley's own `Cargo.toml` depends
on `icu_segmenter` with `features = ["compiled_data"]` unconditionally** —
that feature is not gated behind parley's `complex-scripts` flag at all.
**Turning off `complex-scripts` in Lumen's parley feature selection does not
remove `icu_segmenter_data` (the 12 MB-source data crate) from the dependency
graph.** The ~20 `icu_*` crates the earlier review counted are close to
**irreducible at the Lumen layer** as long as Lumen depends on parley 0.11 at
all for shaping — the fix, if wanted, is upstream (a parley release that
makes `compiled_data` itself optional) or a shaper substitution, not a Cargo
feature flip in Lumen's own manifest. This corrects the campaign's framing
that `complex-scripts` was a free lever; it likely isn't one, today.

**[READ FROM SOURCE]** The GTK cluster (`rfd`, `muda`, `tray-icon`, `gtk`) is
**not behind any Cargo feature at all** — `crates/lumen-shell/Cargo.toml` has
no `[features]` entry for any of the four; they are plain, unconditional
dependencies whenever `lumen-shell` compiles (desktop only — already excluded
from Android/iOS/wasm at the facade level). **This means the documented
"lean" 7.5 MB profile still links GTK3, GLib, and D-Bus** — the lean
profile only drops the font, `serde_json`/snapshot, and image codecs; it does
not touch OS-integration weight at all. This is the single largest unexploited
lever in the whole binary-size story: GX2 (campaign, M-E) scopes a feature
gate for this cluster but it hasn't landed.

### 1.3 A restated, defensible target

Given 1.1 and 1.2, `<5 MB` is not one number to chase — it depends on which
capability you're willing to trade for it:

| Configuration | Plausible floor | Basis |
|---|---|---|
| **Today (lean, GPU + GTK, no font)** | ~7.0-7.5 MB | Measured (7.5 MB, `size_gate.sh`) + cross-checked via `.rodata` decomposition |
| **GPU + GTK removed, lean font** | ~6-7 MB estimate | GTK cluster gate (GX2) not yet built; unmeasured saving |
| **CPU-only (no wgpu), GTK removed, lean font** | plausibly <4 MB | Existence proof: Slint's software-renderer hello-world measured at ~2.8-3.5 MB on Windows (**[DOCUMENTED, third-party]**, not Linux, not re-derived here) |
| **Spec's literal `<5 MB`, full native GPU+dialogs+tray+menu** | **not supported by any measurement in this investigation** | The ~7 MB non-font floor already exceeds it before font/GTK trades |

**Recommendation:** restate the spec target as tiered rather than singular —
`<5 MB` for a CPU-only / no-native-OS-chrome build (the fair comparison to
Slint's MCU-adjacent numbers), and a separate, honestly-labeled `<8 MB` for
the full-featured native-GPU-plus-OS-integration build the "lean" profile
already approximates. This is exactly what LN3 (campaign, M-E) already
half-proposes ("meet it or replace it with the measured floor and a named
reason") — this section is that reason, with the number attached.

---

## 2. Memory

**[MEASURED via a prior review's field-matched reconstruction, not a literal
`size_of` — flagged quarantined by the campaign pending EL0]** `Tree`
(SoA node) ≈ 161 B/node; `Element` (the per-node widget-description struct)
≈ 1008 B. **[MEASURED, negative result, this session]** No
`size_of::<Element>()` compile-time assertion exists anywhere in the source
today (`grep` across `lumen-widgets`/`lumen-core` finds none) — the 1008 B
figure remains an estimate, not a shipped-type measurement, until EL0 lands.

**The more important finding, derived this session from data already in the
repo's own docs:** at realistic UI sizes, per-node structure cost is not the
binding memory constraint — the fixed process/GPU-context baseline is, by
roughly two orders of magnitude.

`docs/comparison-gtk-mintupdate.md` **[DOCUMENTED]** measured `datagrid-win`
(1,041 nodes) at 270 MB RSS, of which ~123 MB is GPU-driver/shader-compiler
residency and ~30 MB is heap (app content). Using the Tree+Element figures
above: `1,041 × (161 + 1,008) ≈ 1.22 MB` — **the entire per-node
representation is ≈0.45% of the measured process footprint.** The same doc
independently observes this directly: "Lumen's memory is essentially a fixed
baseline, not a function of app content" — `counter-win` (a handful of
nodes) uses *more* RSS (292 MB) than the 1,041-node datagrid (270 MB).

**Consequence for the campaign's sequencing:** EL2 (boxing the 11 handler
slots + `LayoutStyle` in `Element`, shrinking it well below 1008 B) is good
hygiene and will matter at very large N (100k+ node scenes, the framework's
own stated virtualization target) or for allocation-churn/CPU-time reasons —
but it will not move the RSS number a user or task manager sees for a
typical app, because that number is currently dominated by the GPU-context
tax (§3), not by node representation. **CP/EL-series work and the GPU-context
work in §3 are solving different problems; only the latter moves "RSS for a
1k-node app" today.**

**A+ bar, proposed:** GTK's own measured toolkit floor on this box
**[MEASURED, prior review]** is 37-49 MB (`python3` + GTK3 import + a
200-row TreeView). A defensible A+ target for a 1k-node Lumen desktop app is
**60-120 MB RSS** — competitive with GTK plus headroom for Rust's larger
static binary and a real (if optional) GPU path — and it is **only reachable
if the GPU-context tax in §3 is closed first**; without that, RSS floors at
216-292 MB regardless of node count, as already measured.

**Unbounded caches** (decoded-image cache, text-editor undo, app-level
`History<T>`, agent session log, `style_memo`) are real but **unfinished, not
structural** — every sibling cache in the same codebase (glyph atlas, GPU
image/tess caches, shadow cache) already uses a proven CAP + half-retention
pattern (`crates/lumen-render/src/gpu.rs:45-52,2120-2126,2437-2443`); the gap
is that five caches didn't get the same treatment, not that the pattern is
missing. **[MEASURED]** Confirmed directly: `crates/lumen-widgets/src/asset.rs:17-19`
(`thread_local! CACHE: RefCell<HashMap<u64, RgbaImage>>`, no cap, no
`clear()`); `crates/lumen-core/src/tree.rs` has no `shrink_to_fit` call site
anywhere in `crates/` (high-water-mark retention, not a leak, but never
released). Effort to close: small, per finding — copy the existing pattern.

---

## 3. Idle power

**[DOCUMENTED, independently corroborated this session at the code level]**
The idle-loop logic itself is correct: `about_to_wait` enters
`ControlFlow::Wait` and is called once in 12 s of idle observation
(`docs/results-idle-and-gpu-context.md`). **[MEASURED]** Confirmed directly:
`ThreadPoolSpawner` workers block on `mpsc::Receiver::recv()` with no
timeout (`crates/lumen-core/src/tasks.rs`), so they cannot be the idle-CPU
source. The residual 0.4-1.9% CPU on this box is the NVIDIA proprietary
Vulkan driver's own 100 Hz polling loop, reproduced at **0 jiffies** on
lavapipe with the identical binary — genuinely not Lumen's bug on *this*
driver. **Caveat the source doc states and this report repeats: one machine,
one vendor.** Mesa (Intel/AMD) and — critically for this report's mobile
question — mobile GPU drivers were never measured. "Not actionable in
Lumen" should not be read as "true everywhere."

**The one genuinely actionable, self-inflicted idle/memory cost found in this
investigation:** **[MEASURED, confirmed directly this session]**
`crates/lumen-shell/src/lib.rs:491-497` and `:1437` — selecting the CPU
(`TinySkia`) renderer does not avoid a GPU context; it *forces a second* one,
because `Presenter::new` unconditionally builds its own
`wgpu::Instance`/`Adapter`/`Device` to blit the CPU-rasterized frame onto the
window (winit does not do presentation itself). Measured cost
**[DOCUMENTED]**: ~123 MB of NVIDIA/LLVM driver residency, paid identically
whether the renderer is "wgpu" or "cpu." The fix (`softbuffer`, presenting a
CPU buffer via X11 SHM/Wayland `wl_shm`/platform equivalents) is fully
scoped in `docs/results-idle-and-gpu-context.md` §2.4 — it is an ADR-003
escalation (new runtime dependency) that has not been proposed as a phase,
not a research problem.

Two more confirmed-directly, small, unfinished items: **[MEASURED]** the
AT-SPI/AccessKit adapter is constructed unconditionally at window creation
(`lib.rs:469-472`), opening a D-Bus session-bus connection regardless of
whether any assistive technology is present, contradicting its own "dormant"
doc comment; and **[MEASURED]** `ThreadPoolSpawner::default()` sizes to
`available_parallelism()` uncapped (32 threads on this 32-core box for an app
that spawns none) — zero idle CPU cost (parked on a blocking `recv`), but
real stack/scheduler bookkeeping that "matters more on a phone than here," as
the source doc itself notes.

**[MEASURED, confirmed directly]** Multi-window compounds the GPU-context tax
rather than sharing it: `Shell::open_secondary`
(`crates/lumen-shell/src/lib.rs:877-925`) calls a fresh
`Wgpu::new()`-equivalent path per window, with no shared Instance/Device —
N windows can hold up to 2N GPU contexts (direct + CPU-fallback presenter,
per window that can't present directly).

---

## 4. Mobile: structural or unbuilt?

This is the decisive question, and the answer is **both, but disentangled
into three separable pieces** — one is a sound, deliberate architectural
choice; one is a plain bug; one is a genuinely open question nobody has
data on.

### 4.1 The rendering choice — sound in isolation, unproven against the stated target

**[MEASURED, confirmed directly this session]** Both mobile shells are
CPU-only. `crates/lumen-shell-android/src/imp.rs`: `hl.pump()` →
`hl.screenshot()` (an `RgbaImage`) → locked directly into the
`ANativeWindow` buffer via `std::ptr::copy_nonoverlapping`; no
`wgpu::Instance` anywhere in the file, and the file's own header comment
says so ("Android-only implementation: native-activity event loop +
software blit"). `crates/lumen-shell-ios/`: `hl.pump(); hl.screenshot()`
returns raw RGBA8 bytes over FFI, and — a correction to the source review
this note was seeded from — the *checked-in* Obj-C template
(`ios/AppDelegate.m`) presents them via **`CGBitmapContextCreate` +
`CGContextDrawImage` (CoreGraphics), not `CAMetalLayer`/Metal**; the
lib.rs doc comment itself says Metal is what "production" would use, not
what's shipped in the template. `.ai_docs/07-decision-log.md:118` records
this as a deliberate T3.1 choice: *"No GPU/wgpu needed on device."*

This is directionally the *right* instinct for an A+ idle-power story on
battery hardware — it matches Slint's own architecture, where the Software
Renderer is the answer for memory/power-constrained targets and is
explicitly designed around damage/partial rendering
**[DOCUMENTED, third-party]**. But there is a real difference worth stating
plainly: Slint reserves its software renderer for genuinely GPU-less MCU
targets and uses a GPU backend (FemtoVG/Skia) on phone-class hardware, which
always has a GPU. Lumen's mobile shells run CPU-only on hardware that *does*
have a GPU, which is a more conservative choice than the closest comparable
project makes for the same class of device.

The framework's own stated target — **[READ FROM SOURCE]**
`.ai_docs/01-architecture.md:70`: "60 fps floor mid-range mobile" — has
**never been measured against this architecture**. Whole-screen CPU
rasterization + a full buffer copy on every changed frame at real phone
resolutions (1080p-1440p, 2-3x DPI) is a materially different cost than a
desktop counter app's occasional redraw. For a mostly-static screen this is
likely fine and cheap. For scroll/animation-heavy content it is an open
question, not a known-good architecture — and it is exactly the CP4 task the
campaign already gates further architecture decisions on, with **"the
explicitly permitted outcome: stop"** if the number comes back bad.
**No ARM device or emulator frame-time measurement exists anywhere in this
repository's history**, per the task graph and this session's search.

### 4.2 The bug — cheap, unfinished, currently paying a cost with no benefit

**[MEASURED, confirmed directly this session]** Despite rendering CPU-only,
both mobile shell crates still **compile `wgpu` into their dependency
graph**, unused. `crates/lumen-render/Cargo.toml`'s `wgpu` dependency is
target-gated only `cfg(not(target_arch = "wasm32"))` — this excludes wasm,
**not** Android/iOS. `cargo tree -p hello_android --target x86_64-linux-android`
(metadata resolution only, no compile) confirms `wgpu`, `wgpu-core`,
`wgpu-hal`, `wgpu-types` resolve into the Android graph via `lumen`'s
default features. This directly contradicts the facade's own doc comment
(`crates/lumen/Cargo.toml:12-13`: *"Web/mobile drop wgpu via target-gating
in lumen-render regardless"* — true for wasm, false for Android/iOS as
written). **This is worst-of-both**: no GPU rendering benefit on mobile,
and the full wgpu dependency-graph weight (build time, and — unmeasured,
since no APK exists — plausibly some binary size even if dead-code-eliminated)
paid anyway. Fix: feature-gate `wgpu` off by default for the mobile example
crates. Effort: trivial, one Cargo.toml change per crate.

### 4.3 The unfinished, cheap part — memory pressure, lifecycle, measurement

**[MEASURED, confirmed directly this session]** Android's `MainEvent` match
in `imp.rs` has every real event arm (`InitWindow`, `RedrawNeeded`,
`WindowResized`, `ContentRectChanged`, `TerminateWindow`, `Destroy`) except
`LowMemory`, which falls into `_ => {}` — received, silently dropped.
**[MEASURED, confirmed directly, full file read]** iOS's `AppDelegate.m` (61
lines) implements exactly 5 Objective-C methods — `drawRect:`,
`dispatchTouch:phase:`, two touch-event handlers, `didFinishLaunchingWithOptions:`,
and `main()` — with `applicationDidReceiveMemoryWarning:`,
`applicationDidEnterBackground:`, and `applicationWillResignActive:` **not
present at all**, not stubbed. **[MEASURED, negative result]** No
`on_memory_pressure`/`clear_caches`/`trim_memory`-shaped function exists
anywhere in the codebase to wire either callback to, even if they were added
today — it would need to be built alongside them (which is exactly what
§2's cache-capping work already needs to build anyway; the two tasks share a
target function).

**[MEASURED]** Same `available_parallelism()`-sized, uncapped thread pool
runs on mobile with no override found anywhere in either shell crate — a
cost that "matters more on a phone" per the framework's own idle-power
investigation, unaddressed on the one platform where it matters most.

**[MEASURED, corrects the seed review's framing]** An APK build pipeline
*does* exist and *has* been exercised, contrary to a blanket "never built"
reading: `scripts/android_build_apk.sh` (cargo-ndk → aapt2 → zipalign →
apksigner, no Gradle) produced an installable APK that was verified on an
API-34 x86_64 emulator, including a `device_golden` test confirming the
on-device screencap matches the headless CPU reference within the project's
own perceptual budget (`.ai_docs/07-decision-log.md:117-121`, dated during
T3.1/T3.2). **What is accurate, and unchanged by this correction:** no APK
artifact exists in the repository or `target/` *today* (find confirms
empty), **no size was ever recorded** at any point (`grep` across the build
scripts for size-reporting logic returns nothing), and
**[MEASURED, confirmed directly]** `.github/workflows/mobile.yml` is
**entirely commented out** — every line — so neither the Android nor the
(never-attempted-beyond-headless) iOS leg runs in CI today. iOS has never
progressed past headless render-core verification on a non-mac host per
`.ai_docs/07-decision-log.md:131`; the simulator path needs macOS, which
this environment doesn't have (matches the user's own prior session note:
"Android emulator-verified; iOS headless-only").

### 4.4 What A+ mobile actually requires, and Lumen's current distance from it

**[BACKGROUND KNOWLEDGE, via the mobile-analysis agent, not repo-specific]**
Flutter forwards Android's `onTrimMemory`/`onLowMemory` and iOS's
`didReceiveMemoryWarning` to `WidgetsBindingObserver.didHaveMemoryPressure()`,
which the framework itself uses to clear its own image cache; React Native
does the analogous thing through its native bridge. All three mainstream
stacks (Flutter, RN, native Swift/Kotlin) tear down or suspend the GPU
surface on backgrounding, and iOS explicitly restricts GPU work while
backgrounded. Lumen has zero of this wired today, on either platform — this
is the concrete gap behind the D grade, not the CPU-render choice.

**[DOCUMENTED, weakly sourced — flagged by the research agent as soft]**
Flutter's Android release APK floor is credibly ~7.7 MB and iOS IPA ~9.1 MB,
from a 2018 first-party Flutter engineering measurement
(`github.com/flutter/flutter/issues/16833`) — the only figure in this whole
competitor search the agent could defend as a real, dated, primary-source
measurement; every more-recent blog figure it found (4.7 MB, 8-10 MB,
"10 MB iOS / 4 MB Android") was inconsistent across sources and explicitly
flagged unverified. **Given Lumen's own non-font floor is already ~7 MB on
desktop before mobile-specific weight (NDK glue, JNI, the unused-but-compiled
wgpu graph) is even added, an A+ mobile binary target in the 5-8 MB range
looks directionally right but cannot currently be defended with a real
number — none exists.**

### 4.5 Answering the question directly

**Not structural.** The CPU-render architecture is a defensible, Slint-adjacent
choice for the idle/static case and is not what's holding mobile at a D. What's
holding it at a D is: zero memory-pressure response (cheap, ~1-2 days once
§2's cache-clear function exists), one dependency-gating bug paying wgpu's
cost for no benefit (trivial), a disabled CI pipeline sitting on top of
toolchain and build tooling that has already been proven to work once, and a
genuine, unresolved uncertainty about whether the rendering architecture
meets the framework's *own* stated performance target — which is not a
structural verdict against the architecture, it is the absence of the one
measurement (CP4) that would turn "plausible" into "proven."

---

## 5. The determinism tax, quantified

**[READ FROM SOURCE, confirmed directly]** `crates/lumen-text/src/lib.rs:285`:
the CPU reference renderer's determinism comes from `system_fonts: false` —
using only fonts the framework itself controls, so shaping never depends on
what happens to be installed on the machine running the test. **This is
independent of font size.** A 355 KB Latin-only face and a 15.5 MB
pan-Unicode face are equally deterministic under this contract; the campaign
plan already makes this point (`zippy-dancing-allen.md`: "Determinism comes
from `system_fonts: false`... not from the face being 15,515,760 bytes") and
this session's source read confirms it directly.

**What the 15.5 MB font actually buys** is CJK/RTL/Indic glyph *coverage*
for the framework's own golden tests and i18n example apps
(`crates/lumen-text/Cargo.toml`'s own comment: "CJK/RTL/Indic goldens and
the i18n examples rely on it") — a test-corpus decision that has been
conflated with the determinism contract, but is a separate cost:

| Cost | Bytes | What it buys | Who pays it |
|---|---|---|---|
| **Determinism itself** | ~527 KB (`GoNotoKurrent-Latin.ttf` 354,748 B + `DejaVuSans-Symbols.ttf` 171,732 B, **[MEASURED]**) | Bit-identical CPU rendering for any Latin-script app, forever | Every shipped app, unavoidably, if it wants golden tests at all |
| **CJK/RTL test coverage** | 15,515,760 − 354,748 = **15,161,012 B (≈14.5 MB)** | The framework's own i18n golden tests + example apps | Currently: every shipped app, by default. Necessarily: only Lumen's own CI and any app that actually ships CJK/RTL text |

**Golden PNG files themselves are negligible**: **[MEASURED]**
`du -sh` across the five `tests/golden` directories in the workspace
(`lumen-widgets`, `lumen-test`, `lumen-text`, `lumen-render`, `hello`) totals
376 KB. The tax is entirely in the font, not the fixtures.

**Could goldens use a demand-fetched font in CI only, shipping nothing?**
Yes, and it does not weaken the determinism contract to do so. Determinism
requires the *same pinned font bytes* on every test run, not that those
bytes live inside the shipped `lumen-text` library crate. A
checksum-pinned CJK font vendored into a test-only or dev-dependency-scoped
crate (or fetched-and-cached by a build script gated to `cfg(test)`/example
builds) preserves bit-identical goldens while removing the bytes from
`lumen-text`'s own `include_bytes!` path that every consumer inherits. This
is functionally what LN2 (campaign, M-E: "flip defaults, gate CJK behind
explicit opt-in") already proposes; this section's contribution is
confirming the split does not compromise ADR-005/the determinism contract,
and separating "determinism tax" (527 KB, real, universal) from
"i18n-test-coverage tax" (14.5 MB, avoidable for the ~95% of apps that never
render CJK/RTL) as two different numbers, because the current doc language
conflates them.

**[READ FROM SOURCE]** `tiny-skia` (the CPU reference renderer itself) is an
unconditional dependency in every profile, lean or not — its own binary-size
contribution was not isolated in this session (no build was run), but it is
a small, focused 2D raster crate, not a line item competitive with the font
or GTK cluster.

---

## 6. The GTK linkage

**[MEASURED, confirmed directly this session]** `ldd target/release/examples/counter-win`:
real dynamic links to `libgtk-3.so.0`, `libglib-2.0.so.0`, `libdbus-1.so.3`,
`libX11.so.6`, `libwayland-{client,cursor,egl}.so.*`. Not a Cargo-graph
presence — a real runtime link, on every desktop Linux build, unconditionally
(§1.2).

**[READ FROM SOURCE]** The decision log shows this was a considered trade at
each step, not an oversight:

- **rfd** (P.3b, 2026-07-11): the xdg-portal backend "requires a full async
  runtime (tokio or async-std) inside `lumen-shell`" — ruled out under
  ADR-003 (no bundled async runtime outside agent/dev-server scope); GTK3
  chosen instead, with a stated revisit condition ("if the GTK linkage leaks
  beyond `lumen-shell` or a portal-only environment matters").
- **muda** (P.3c): "muda's Linux `platform_impl` *is* GTK — the crate
  doesn't compile there without the `gtk` feature (verified)." **[MEASURED,
  confirmed independently this session]** reading muda 0.19.3's actual source
  (`src/platform_impl/mod.rs`) shows the Linux platform module is gated
  `#[cfg(all(target_os="linux", feature="gtk"))]` with **no non-gtk Linux
  module in the crate at all** — this specific claim holds up.
- **tray-icon** (P.3e): shares muda's gtk dependency; a field finding in the
  same entry notes ayatana-appindicator silently refuses to register a
  StatusNotifierItem without a menu, which is *why* the tray hosts the app
  menu on Linux (the one place a "native menu" benefit materializes there).
  The same entry records: **"no pure-Rust muda replacement exists"** — as of
  2026-07-19.

**What's changed since, found by this session's competitor research:**
**[DOCUMENTED, third-party, this session's web research]** rfd 0.14.1's
*upstream default* is now `["xdg-portal", "wayland"]` — `gtk3` is the
opt-in, non-default feature. Lumen's own `Cargo.toml:143` explicitly
overrides this back to `gtk3` (`rfd = { version = "0.14", default-features
= false, features = ["gtk3"] }`). And rfd's `xdg-portal` feature pulls
`ashpd` + `pollster`, not `tokio`/`async-std` — `pollster` is a minimal
blocking-poll executor, not the "full async runtime" the 2026-07-11 decision
was written against. **This is the single clearest revisitable decision in
this whole report**: the specific technical objection that justified
choosing GTK for file dialogs appears to no longer describe rfd's current
architecture, and re-testing the portal path against ADR-003 is cheap
relative to the payoff (it removes GTK's *first* and most defensible
justification).

**[DOCUMENTED, third-party]** `ksni` is a genuine pure-Rust
StatusNotifierItem tray implementation with no GTK/libappindicator
dependency (uses `zbus` + `tokio`/`async-io`) — **[MEASURED, negative
result]** not vendored, mentioned, or evaluated anywhere in this repo's
docs/backlog today. It carries its own ADR-003 conversation (an async
runtime dependency), and StatusNotifierItem support is desktop-environment-
dependent (GNOME historically needed an extension) — a real integration
cost, not a drop-in.

**Net assessment:** menus never had a native-Linux-menubar payoff to begin
with (already documented: winit has no menubar attachment point on Linux
regardless of GTK), so the *only* remaining Linux justification for the GTK
cluster is the tray icon. If tray moves to `ksni` (moderate effort +
one governance conversation), GTK's remaining justification on Linux
evaporates, and file dialogs can very plausibly move off it too given the
upstream default change. **"Pure Rust" is recoverable on Linux with moderate,
bounded effort — it is not a research problem, but it does require accepting
a new dependency (`ashpd`/`pollster` for dialogs, `zbus`+an async runtime for
tray) in place of the one being removed, so it is a trade, not a pure
subtraction.**

---

## 7. Web/wasm

**[DOCUMENTED, not reproduced this session]** `scripts/web_gate.sh` gates
wasm size at ≤24 MB; the last recorded run was 22 MB
(`.ai_docs/07-decision-log.md:427`), with the same pan-unicode font as
desktop baked in unconditionally — **[READ FROM SOURCE, confirmed]**
`crates/lumen-shell-web/Cargo.toml` has no font-feature override, and
`lumen-render`'s wgpu dependency is correctly excluded on `wasm32` at the
Cargo target-cfg level (the wasm build is already CPU-only, unlike mobile's
accidental-inclusion bug in §4.2).

**No wasm build exists locally to re-measure** (`target/` has no
`wasm32-unknown-unknown` directory at all) — building one was out of scope
under the disk constraint. **[ESTIMATE/INFERENCE]**: applying the same
font-floor logic as §1 — cut the 15.5 MB font, and wasm's remaining floor
should land *below* desktop's ~7 MB non-font floor (wgpu/lyon/ash/glow are
never compiled for wasm at all), plausibly in the 3-5 MB uncompressed range,
gzipped meaningfully smaller. **This is an inference, not a measurement —
flagged explicitly per the task's instruction not to present an estimate as
one.**

**[DOCUMENTED, third-party, competitor research]** No comparator number in
this space held up to scrutiny: the one concrete data point found was a
Dioxus GitHub issue where a maintainer-claimed "~65 KB" hello-world bundle
was contradicted by a real user's measured 275 KB build
(`github.com/DioxusLabs/dioxus/issues/732`) — a 4x gap, and the smaller
number's build recipe was never documented. egui/eframe wasm size, Yew, and
Leptos hello-world bundle sizes all returned **no defensible primary-source
number** despite a genuine search effort. **This report cannot give Lumen's
wasm path a competitive A+ target with the rigor this document tries to hold
everywhere else — it can only say the same font fix that helps desktop
almost certainly helps wasm by a similar proportion, and that the campaign's
LN-series doesn't currently call out re-measuring `web_gate.sh`'s 24 MB
budget once the font flips, which is worth adding.**

---

## 8. Build-time resources

**[MEASURED]** `target/` = 71 GB (`du -sh`): `debug` = 68 GB (of which
deps ≈ 48 GB, incremental ≈ 12 GB per the prior review's count, not
re-verified byte-for-byte this session), `release` = 2.3 GB,
`x86_64-linux-android` = 752 MB, `doc` = 15 MB. **[MEASURED]** 71 workspace
members total, 51 under `examples/` (counted directly from `Cargo.toml`'s
`members = [...]` block this session — close to, and superseding, a prior
review's 73/51 count, likely just workspace drift between sessions).

**[READ FROM SOURCE]** `.cargo/config.toml` sets `CARGO_INCREMENTAL = "0"`
as a disk-pressure mitigation after `target/` regrew to 151 GB once
(2026-07-11) — a real, acknowledged trade against "fast iteration for an
AI-first framework," since it means every non-`.lss` recompile is a
from-scratch compile for every crate touched. **[DOCUMENTED, prior review,
not re-verified this session]**: some incremental-cache subdirectories
postdate the mitigation commit, an unresolved discrepancy possibly
indicating the env var isn't fully taking effect (needs `cargo build -vv`
to confirm — not run this session, out of scope for a non-compiling
investigation).

**[READ FROM SOURCE]** Feature unification is real and independently
confirmed twice this session (once via the modularity review, once via the
mobile agent's Android-specific check): `cargo build --workspace` unifies
Cargo features across all ~71 members, so the "lean" paths in `lumen-core`/
`lumen-style`/`lumen-widgets` are *always* compiled with every feature on in
CI; the only place the lean profile is actually exercised is
`size_gate.sh`'s throwaway out-of-workspace crate, which never runs
`cargo test`.

**A+ build-time story, proposed:**
1. Verify `CARGO_INCREMENTAL=0` is actually taking effect (small, diagnostic
   task — `cargo build -vv`, check the invoked rustc's env).
2. Evaluate `sccache` (not currently configured anywhere) as a
   corruption-safer alternative to incremental caching — this is the
   project's own F12 recommendation, not new to this report.
3. A per-crate `cargo build -p <crate>` dev loop (what an AI agent doing
   single-crate iteration would actually run) does not pay the full-workspace
   feature-unification cost `cargo build --workspace`/CI pays — the 70 GB
   figure is largely a symptom of CI-style whole-workspace builds plus 51
   example crates each carrying their own `target/debug/deps` entries, not
   of the single-crate dev loop the "fast iteration" pitch is actually about.
   Worth verifying directly (this session did not build anything, so this is
   **[ESTIMATE/INFERENCE]** from reading Cargo's documented per-invocation
   feature-resolution behavior, not measured).
4. `lumen-widgets` splitting (already recommended by the modularity review
   for API reasons) would shrink the blast radius of incremental recompiles
   as a side effect — a 26k-LOC, 18%-runtime-code single crate means touching
   `app.rs` recompiles everything downstream of it.

---

## 9. Blocker analysis — per axis

| Axis | Classification | Why |
|---|---|---|
| Binary size — bundled CJK font | **Unfinished** | LN1/LN2 already scoped; proven pattern (feature-gate) exists |
| Binary size — GTK cluster unconditional | **Unfinished** | GX2 scoped, not built; no technical blocker found |
| Binary size — GTK cluster *existence* (vs. pure-Rust alternatives) | **Revisitable decision** | rfd's upstream default changed since the 2026-07-11 ruling; muda/tray-icon genuinely need GTK today, ksni is untried |
| Binary size — wgpu backend restriction (single HAL backend) | **Unfinished, small** | Feature flag not set; saving unmeasured |
| Binary size — wgpu itself (drop for CPU-only) | **Revisitable decision, real trade** | Removes GPU rendering entirely; big size win, real capability cost |
| Binary size — ICU/`complex-scripts` | **Mostly irreducible at Lumen's layer** | Baked into parley's own manifest (`icu_segmenter/compiled_data` unconditional); needs an upstream change or shaper substitution |
| Binary size — duplicate crate versions | **Mostly irreducible** (downstream of GTK/AT-SPI ecosystem clash), **except** the swash+parley double font-stack, which is Lumen's own choice | dedup opportunity is real and Lumen-controlled |
| Memory — unbounded caches (F1/F2/F8/F9/F10) | **Unfinished** | Proven CAP+half-retention pattern exists three times already in the same codebase |
| Memory — `Element` struct size | **Unfinished, currently unmeasured** (quarantined pending EL0) | Real but secondary to §3's fixed baseline at typical UI sizes |
| Memory — GPU-context dominates RSS | **Unfinished, fully scoped, not built** | softbuffer path designed in `docs/results-idle-and-gpu-context.md`, needs an ADR-003 escalation decision, not research |
| Memory — multi-window GPU duplication | **Unfinished** | Standard shared-device wgpu pattern, not novel engineering |
| Idle CPU — driver residency (this machine) | **Irreducible on this driver**, unverified elsewhere | Explicitly single-machine/single-vendor caveated by the source doc itself |
| Idle CPU — AccessKit thread always-on | **Unfinished, contingent on upstream** | Needs `accesskit_unix` to support lazy attach; unverified either way this session |
| Idle CPU — thread pool sizing | **Unfinished, trivial** | `min(4,cpus)` fix already scoped (CACHE1) |
| Mobile — memory-pressure wiring | **Unfinished, cheap** | MOB1/MOB2 scoped; shares a target function with the desktop cache work |
| Mobile — wgpu compiled-but-unused | **Unfinished bug** | One Cargo.toml feature-gate change |
| Mobile — CPU-render architecture vs. the 60 fps target | **Genuinely open, not proven either way** | Gated on CP4, a measurement that has never been taken |
| Mobile — CI/build pipeline | **Unfinished, not structural** | Toolchain proven to work once (T3.1/T3.2); CI file fully commented out |
| Determinism tax | **Revisitable, low-risk** | CJK coverage ≠ determinism itself; splitting them doesn't touch the determinism contract |
| Web/wasm | **Unfinished, unmeasured** | Same font fix applies; no wasm-specific re-gate scoped anywhere yet |
| Build time | **Unfinished + one standing trade** (`CARGO_INCREMENTAL=0`) | sccache evaluation pending; workspace/crate-split would help incrementally |

---

## 10. The path — ordered, costed

**Phase 0 — cheap, already scoped, mostly inside the campaign's own M-E
(1-2 weeks).**
1. Flip `pan-unicode` default off; move the CJK/RTL face to a test/example-
   only asset per §5's split (LN1 needs a real, reproducible subsetting
   script — none exists today; LN2 flips the defaults).
2. Cap the five unbounded caches with the existing proven pattern (CACHE1).
3. Right-size the thread pool (`min(4, cpus)` or lazy-grow) on desktop *and*
   mobile.
4. Feature-gate `wgpu` off for the mobile example crates (§4.2's bug) —
   trivial, no downside.
5. Wire Android `LowMemory` + an iOS memory-warning delegate method to the
   cache-clear function built in step 2 (MOB1/MOB2).
6. Defer AccessKit adapter construction if `accesskit_unix` supports it;
   otherwise correct the "dormant" doc comment (GX4).

*Outcome:* desktop binary reaches the already-documented ~7.5 MB lean floor;
RSS improves modestly; mobile gains real memory-pressure response for the
first time.

**Phase 1 — the GPU-context work, fully scoped, not yet built
(3-5 weeks).**
7. Build the `SoftPresenter`/softbuffer CPU-present path (ADR-003
   escalation) — closes the ~123 MB GPU-context tax for idle/hidden/
   CPU-rendered windows. This is the single highest-leverage item in this
   entire report for desktop memory and idle power simultaneously.
8. Share one `wgpu::Instance`/`Device` across all windows instead of one
   (or two) per window.
9. Add `Occluded`/`suspended` handling to tear down GPU resources on
   minimize/background — directly reusable on mobile once/if it gets a real
   GPU path (Phase 3).
10. Restrict `wgpu-hal` to a single backend feature per platform.

*Outcome:* desktop RSS floor plausibly drops from the measured 216-292 MB
range toward the 60-120 MB target proposed in §2, for idle/simple apps.

**Phase 2 — the GTK-cluster decision, moderate effort, currently declined
by the campaign (4-8 weeks, ecosystem-testing-bound not code-bound).**
11. Re-test rfd's `xdg-portal` (ashpd/pollster) backend against ADR-003's
    current scope — the objection that blocked it in 2026-07-11 may no
    longer hold.
12. Evaluate `ksni` for the Linux tray (new ADR-003 async-runtime
    conversation + desktop-environment compatibility testing).
13. Drop `muda`'s `gtk` feature on Linux once tray no longer needs it —
    Linux never got a native menubar from it anyway.
14. Feature-gate the remaining GTK-cluster surface (GX2) so a build can
    exist with zero `gtk`/`dbus` linkage, `ldd`-verified.

*Outcome:* "pure Rust" becomes literally true on Linux for a build that
opts in; binary floor drops further (and duplicate-crate churn drops with
it, since most of it traces to the GTK/zbus ecosystem clash in §1.2).

**Phase 3 — the one genuinely open question (unknown effort, gated on a
measurement).**
15. CP4 — real ARM device/emulator frame-time measurement of the CPU-blit
    mobile render path against the 60 fps mid-range target. If it passes:
    mobile's architecture is vindicated and A+ is a matter of finishing
    Phase 0-style work. If it fails: mobile needs an actual GPU path (wiring
    wgpu's already-workspace-present Vulkan/GLES/Metal backends into two
    shells that have never used them, plus all of Phase 1's lifecycle work
    under mobile's tighter memory/battery budget) — potentially large,
    and the only item in this report that could not be estimated with any
    confidence, because the deciding measurement doesn't exist yet.
16. Re-enable `.github/workflows/mobile.yml`, build one real APK and one
    real IPA (MOB3), and add size/cold-start gates — the toolchain has
    already been proven to work once; this is re-enabling and hardening,
    not building from scratch.

*Outcome after Phases 0-2:* desktop plausibly A- to A, depending on which
GTK/GPU trade is accepted. *Outcome after Phase 3:* mobile reaches B/B+
unconditionally (Phase 0 alone fixes the D-grade gaps that are pure neglect)
and A- to A only if CP4 vindicates the current architecture — genuinely
undetermined until that measurement exists.

---

## Measured vs. estimated

| # | Claim | Status | Source |
|---|---|---|---|
| 1 | `hello` release binary = 23,151,024 B | **[MEASURED]** | `ls -la target/release/hello`, this session (pre-existing artifact, not rebuilt) |
| 2 | `.rodata`=19,853,448 B / `.text`=2,823,398 B / `.eh_frame`=163,948 B / `.data.rel.ro`=99,008 B | **[MEASURED]** | `size -A target/release/hello`, this session |
| 3 | Font bytes = 67.8% of binary, 79.0% of `.rodata`; non-font floor ≈ 7.46 MB | **[MEASURED]**, computed from #1+#2 | This session |
| 4 | Font file sizes (15,515,760 / 354,748 / 171,732 B) | **[MEASURED]** | `ls -la crates/lumen-text/fonts/`, this session |
| 5 | Lean profile = 7.5 MB | **[DOCUMENTED]**, not reproduced | `scripts/size_gate.sh:6` comment |
| 6 | wgpu subtree, Linux host resolution = 63 unique packages (prior review's differently-scoped count: 97) | **[MEASURED]**, both counts | `cargo tree -p wgpu`, this session and prior review, different flattening methods |
| 7 | Non-Linux HAL backends (metal/d3d12) absent from Linux-host `cargo tree`, present under `--target` | **[MEASURED]** | `cargo tree -p wgpu --target=<foreign>`, this session |
| 8 | `icu_segmenter/compiled_data` is unconditional in parley's manifest, not gated by `complex-scripts` | **[READ FROM SOURCE]** | `~/.cargo/registry/…/parley-0.11.0/Cargo.toml`, this session |
| 9 | parley 0.11 does not panic without `complex-scripts` (doc comment is stale) | **[READ FROM SOURCE]** | parley 0.11.0 source, `analysis/mod.rs`, this session |
| 10 | GTK cluster is not behind any Cargo feature | **[READ FROM SOURCE]** | `crates/lumen-shell/Cargo.toml`, this session |
| 11 | `ldd` shows real libgtk-3/libglib/libdbus/libX11/libwayland links | **[MEASURED]** | `ldd target/release/examples/counter-win`, this session |
| 12 | rfd 0.14.1 upstream default is `xdg-portal`, not `gtk3`; Lumen overrides back to `gtk3` | **[DOCUMENTED, third-party]** + **[READ FROM SOURCE, Lumen's own Cargo.toml]** | rfd's Cargo.toml on GitHub + `Cargo.toml:143`, this session |
| 13 | muda 0.19.3 genuinely doesn't compile on Linux without `gtk` | **[READ FROM SOURCE]** | muda 0.19.3 registry source, this session |
| 14 | ksni exists, pure-Rust, not evaluated in this repo | **[DOCUMENTED, third-party]** + **[MEASURED, negative result]** | GitHub/docs.rs + repo-wide grep, this session |
| 15 | Android CPU-blit render path, no wgpu construction in `imp.rs` | **[MEASURED, confirmed directly]** | `crates/lumen-shell-android/src/imp.rs`, this session |
| 16 | iOS template presents via CoreGraphics, not Metal (corrects the seed review) | **[MEASURED, confirmed directly]** | `crates/lumen-shell-ios/ios/AppDelegate.m`, this session |
| 17 | wgpu resolves into the Android dependency graph despite being unused | **[MEASURED]** | `cargo tree -p hello_android --target x86_64-linux-android`, this session |
| 18 | Android `LowMemory` unhandled; iOS has no memory-warning method at all | **[MEASURED, full-file reads]** | `imp.rs` match block + `AppDelegate.m` full text, this session |
| 19 | No APK/IPA/xcodeproj exists in the repo today | **[MEASURED]** | `find`, this session |
| 20 | An APK *was* built and device-verified historically (T3.1/T3.2) | **[DOCUMENTED]** | `.ai_docs/07-decision-log.md:117-121`, not reproduced this session |
| 21 | `mobile.yml` is fully commented out | **[MEASURED]** | file read, this session |
| 22 | Tree=161 B/node, Element=1008 B/node | **[DOCUMENTED, field-matched reconstruction]**, quarantined pending EL0 | Prior review; `size_of::<Element>()` assertion confirmed absent this session |
| 23 | 1,041-node datagrid RSS=270 MB; per-node structure ≈1.22 MB ≈0.45% of that | **[DOCUMENTED]** RSS figure + **[MEASURED]** the derived ratio, this session | `docs/comparison-gtk-mintupdate.md` + this session's arithmetic |
| 24 | Idle CPU 0.4-1.9%, driver-caused, 0 jiffies on lavapipe | **[DOCUMENTED]**, not reproduced | `docs/results-idle-and-gpu-context.md` |
| 25 | CPU renderer still constructs a second wgpu device (Presenter) | **[MEASURED, confirmed directly]** | `crates/lumen-shell/src/lib.rs:491-497,1437`, this session |
| 26 | Multi-window: no shared Instance/Device across windows | **[MEASURED, confirmed directly]** | `crates/lumen-shell/src/lib.rs:877-925`, this session |
| 27 | AccessKit adapter constructed unconditionally at window creation | **[MEASURED, confirmed directly]** | `crates/lumen-shell/src/lib.rs:469-472`, this session |
| 28 | Determinism = `system_fonts: false`, independent of font size | **[MEASURED, confirmed directly]** | `crates/lumen-text/src/lib.rs:285`, this session |
| 29 | Golden PNGs total 376 KB across 5 crates | **[MEASURED]** | `du -sh`, this session |
| 30 | 71 workspace members, 51 under `examples/` | **[MEASURED]** | `awk`/`grep` on `Cargo.toml`, this session |
| 31 | `target/` = 71 GB (debug 68 / release 2.3 / android 0.75 / doc 0.015) | **[MEASURED]** | `du -sh`, this session |
| 32 | wasm bundle = 22 MB, budget 24 MB | **[DOCUMENTED]**, not reproduced (no wasm build exists to re-check) | `.ai_docs/07-decision-log.md:427` |
| 33 | Slint hello-world ≈2.8-3.5 MB (software renderer, Windows) | **[DOCUMENTED, third-party, unverified methodology on Linux]** | GitHub discussion #9570, no independent reproduction |
| 34 | Flutter Android APK floor ≈7.7 MB / iOS IPA ≈9.1 MB (2018 figure) | **[DOCUMENTED, third-party, dated/primary but old]** | `github.com/flutter/flutter/issues/16833` |
| 35 | egui/eframe native or wasm hello-world size | **[UNVERIFIABLE]** — no defensible number found by either agent | Explicitly flagged, not estimated |
| 36 | Dioxus/Yew/Leptos wasm hello-world size | **[UNVERIFIABLE]**, one contradictory data point (65 KB claimed vs. 275 KB measured for Dioxus) | GitHub issue #732, explicitly flagged |
| 37 | Lumen wasm non-font floor plausibly 3-5 MB uncompressed | **[ESTIMATE/INFERENCE]** — not measured, no wasm build run | This session's reasoning from §1's desktop floor minus wgpu weight |
| 38 | A+ mobile RSS/binary targets | **[ESTIMATE/INFERENCE]**, grounded in Flutter's one solid figure and Lumen's own desktop floor, explicitly not a measurement | This session |

---

## What this note adds beyond the seed review and the campaign

1. **The non-font floor (~7 MB, cross-validated two independent ways) is the
   single most consequential number in this report** — it proves `<5 MB` is
   unreachable by font policy alone, which neither the seed review nor the
   campaign states this precisely.
2. **`complex-scripts` is not the ICU-weight lever it's assumed to be** —
   `icu_segmenter/compiled_data` is unconditional in parley's own manifest.
   This corrects a load-bearing assumption in the campaign's framing.
3. **The lean profile still links GTK3/D-Bus** — "lean" only ever meant
   "no CJK font, no snapshot," never "no OS-toolkit weight." This wasn't
   stated plainly anywhere read this session.
4. **rfd's upstream default changed since the GTK decision was made** —
   the specific technical objection (needs a full async runtime) that
   justified choosing GTK for file dialogs appears to no longer describe
   rfd's current architecture. This is a concretely revisitable decision
   with a cheap first step (re-test the portal backend).
5. **Per-node memory cost is not the binding constraint at realistic UI
   sizes** — the GPU-context tax dwarfs it by ~200x at 1,041 nodes. This
   reprioritizes the campaign's CP/EL-series relative to its M-E series for
   anyone optimizing for *user-visible RSS* specifically, as opposed to
   allocation-churn/CPU-time.
6. **Mobile's D grade is disentangled into three different problems with
   three different costs** — a sound-but-unproven architecture choice, one
   trivial bug, and a pile of cheap unfinished plumbing — rather than one
   monolithic "mobile is behind" verdict. The genuinely open question (CP4)
   is isolated from the parts that just need someone to do the work.
7. **A corrected mobile rendering claim**: the iOS template presents via
   CoreGraphics, not Metal — the seed review's phrasing ("hands back raw
   RGBA bytes for an external Metal template") was read as implying Metal is
   what's shipped; it isn't, a comment in the source says so, and this
   matters for anyone estimating what a real GPU path on iOS would cost to
   add (it isn't "swap the presenter," it's "build one").
