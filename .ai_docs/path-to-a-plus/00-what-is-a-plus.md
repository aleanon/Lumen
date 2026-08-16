# What "A+" means for a GUI framework — and where Lumen sits

*Research study, 2026-08-06/07. Foundational document for four sibling
reviews (performance, consumer API, modularity, resource usage) assessing
whether Lumen can reach A+ on each dimension. Method: read Lumen's own
2026-08 five-domain review (`.ai_docs/review-2026-08/*`), its self-falsifying
benchmark results (`docs/results-node-cost-n0.md`,
`docs/results-idle-and-gpu-context.md`, `docs/comparison-gtk-mintupdate.md`),
plus four parallel web-research passes covering frame time/scroll, startup/
binary size, memory/idle-power, and hot-reload/API ergonomics across eleven
competitor frameworks. Every number below is labeled with its evidence class
— **MEASURED** (someone ran a reproducible benchmark), **VENDOR-PUBLISHED**
(official docs/blog claims a target or result), or **ESTIMATED/ANECDOTAL**
(forum impression, no real instrument) — and its source. Where no reliable
number exists, that gap is stated as a finding, not filled with a guess.*

---

## 0. The one-line result

Lumen already **clears the Rust-native A+ bar on startup latency** and has,
by a wide margin, **the most rigorous self-measurement culture of any
framework examined** — no competitor publishes a field-matched per-node byte
cost or a bench designed to falsify its own architecture's central claim.
But on almost every other numeric axis — binary size, idle memory, and
above all the flagship "incremental rebuild" mechanism — Lumen is currently
**behind its own stated targets, not just behind competitors**, and the
review's own instruments proved it. The good news embedded in that: nearly
every numeric gap found is a **gap of degree** (a bug or a missing feature
flag, fixable without new architecture) rather than a **gap of kind** (a
wall that would require rearchitecting). The one place a real gap of kind
exists — Tier-2 code hot-reload matching Flutter/Compose's seamlessness — is
also the one place *no* Rust AOT framework has closed it, so Lumen isn't
behind its actual peer group there either.

---

## 1. The comparison set

Twelve candidates were named in the brief. Not all are equally fair
yardsticks for "a native Rust desktop+mobile framework whose primary user is
an AI agent" — using them uncritically produces exactly the kind of
apples-to-oranges number Lumen's own docs already warn against (`docs/
comparison-gtk-mintupdate.md §8`: "Lumen is not competing with GTK 3 on GTK
3's terms"). Splitting them by what they're actually useful for:

| Tier | Frameworks | Why they're the right yardstick, and for what |
|---|---|---|
| **A — direct architectural peers** (compiled, retained/fine-grained, no VM) | **Slint, Makepad** | The *only* two frameworks Lumen's own design docs cite as the motivating comparison (`plan-node-cost.md`'s "Makepad's cost model"), and the only ones structurally required to solve the same problem Lumen has (O(changed) updates without a JIT to lean on). Fair for performance, binary size, startup, memory. **Never benchmarked against Lumen — the review's own single biggest gap.** |
| **B — same-toolchain-class Rust competitors** (share Lumen's binary-size floor, no VM, same wgpu/GPU-backend tradeoffs) | **egui, iced, Dioxus** | Different paradigms (egui: immediate mode; iced: Elm-architecture retained; Dioxus: virtual-DOM) but same compilation model, same "you own your GPU backend" cost structure. Fair for binary size, startup, idle memory. iced is already the *implicit* comparison Lumen's own API review uses informally — worth making explicit and numeric. |
| **C — mature native toolkits** (the "how does a serious, shipped, non-Rust native toolkit do this" bar) | **GTK4, Qt/QML** | GTK4 (via `gtk4-rs`, not Python/GTK3) is the fair, cheap-to-run version of the comparison Lumen already partially did (`docs/comparison-gtk-mintupdate.md`, which the project's own doc explicitly disclaims as "not evidence against real compiled competitors," §8). Qt/QML is the other mature retained-mode C++ toolkit with decades of layout/paint optimization and a real mobile-shipping history. Fair for memory, startup, frame time; **not fair** for hot-reload (both have real but narrower live-reload than Lumen's `.lss` tier) or agent-observability (neither has any). |
| **D — best-in-class managed/JIT frameworks** (the ergonomics and hot-reload *ceiling*, explicitly not a fair performance fight) | **Flutter, Jetpack Compose, SwiftUI** | Structurally advantaged on hot reload (Dart JIT VM / JVM bytecode instrumentation / Xcode dynamic replacement) and on some ergonomics axes (property wrappers, smart recomposition) in ways a Rust AOT binary cannot replicate without a different architecture. Use these to set the *qualitative* API-ergonomics and hot-reload bar, and explicitly exclude them from binary-size/startup "fair fight" framing — Lumen's own architecture doc already correctly frames Flutter's AOT engine as "the heavyweight bound," not a peer. |
| **E — recently-modernized cross-platform peer** | **Avalonia** | .NET/XAML-family, but AOT-capable (NativeAOT) and mid-optimization-pass right now — its own public numbers (idle CPU 0.20%→<0.01%, cold start 1960ms→460ms via NativeAOT) describe a framework going through almost exactly the transition Lumen needs to go through. Useful peer-in-time, lower priority than tiers A–C. |
| **F — the floor** | **React/Blink (web)** | Included specifically as the "acceptable to ship" floor: 16.6ms frame budget, Core Web Vitals INP ≤200ms at p75. Useful to confirm Lumen clears even the low bar, and because Tauri/Electron (React/Blink's desktop-shell cousins) are what Lumen is implicitly competing against for a chunk of its adoption. |

**Excluded from the numeric yardstick entirely, included only where named:**
Dioxus and Makepad have almost no public numeric data at all (confirmed
below) — they stay in the qualitative hot-reload comparison (where Makepad
in particular is directly relevant) but contribute nothing to the numeric
bars.

---

## 2. The A+ bar — numeric

Every row states the evidence class. Where research found **no rigorous
number for a framework**, that is stated explicitly rather than
interpolated — this happens more often than a reader might expect, even for
mature frameworks (GTK4 and Qt/QML have essentially no published
frame-time-percentile data despite decades of shipping).

### 2.1 Frame time / jank on a realistic app

| Framework | p50 | p99 | Evidence | Source |
|---|---|---|---|---|
| Android platform bar (any framework) | — | 60Hz: 16ms threshold; 90Hz: 11ms; 120Hz: 8ms. "Slow frame" flagged if >25% of sessions exceed it; "frozen frame" if >0.1% exceed 700ms. | VENDOR-PUBLISHED | [Android Vitals — render](https://developer.android.com/topic/performance/vitals/render), Google, current |
| Jetpack Compose | 4.3–5.4ms | 33.1–57.5ms (unoptimized) → 41.8ms (after optimization pass) | MEASURED (Pixel 3a emulator, community macrobenchmark) | [Compose vs View perf](https://github.com/e-Garcia/Compose-vs-Android-View-System-Performance), 2024; official methodology at [Compose Hero Benchmarks](https://developer.android.com/develop/ui/compose/performance/herobenchmark) |
| SwiftUI | — | — | VENDOR-PUBLISHED, qualitative only | WWDC 2025: render time 16.7ms→10.2ms in iOS 26 (a *relative* improvement claim, not a percentile) — [WWDC25 notes](https://dev.to/softwaretechpro/wwdc-2025-optimize-swiftui-performance-with-instruments-4o4j) |
| Flutter | — | — | No p50/p99 found | Vendor claims "sustains 60fps" under load; no percentile data published |
| Qt/QML | — | — | **No data found** | Target stated as "<16ms/frame" ([Qt Quick performance docs](https://doc.qt.io/qt-6/qtquick-performance.html)); `qmlbench` tool exists but no published results |
| GTK4 | — | — | **No data found** | `gtk4-rendernode-tool` benchmark mode exists ([GTK docs](https://docs.gtk.org/gtk4/gtk4-rendernode-tool.html)); no published numbers |
| egui | ~1–2ms typical | — | MEASURED (informal) | Framework/community reports, 200–400fps in practice — [Tauri/Iced/egui comparison](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html), 2023 |
| Slint | — | — | **No steady-state data**; only a resize-stress anecdote (10–15fps) | [Rust GUI benchmark](https://medium.com/@build_break_learn/rust-gui-framework-benchmark-egui-iced-slint-gtk-electron-d88596c042fb) |
| Dioxus, Makepad | — | — | **No data found for either** | — |
| Avalonia | — | — | 1,867% FPS improvement claimed for a 350k-element scene (before/after not disclosed); Android scroll 42→120fps after an optimization pass | VENDOR-PUBLISHED — [Avalonia 12 release](https://avaloniaui.net/blog/avalonia-12) |
| React/Blink (floor) | 16.65ms avg | 16.75ms | MEASURED, right at budget (little headroom) | [Core Web Vitals](https://www.corewebvitals.io/core-web-vitals); INP ≤200ms at p75 is Google's pass bar |

**Synthesized A+ bar** (derived from the Android Vitals thresholds + the one
rigorous percentile dataset found, Compose's): **desktop p50 ≤6–8ms, p99
≤12–14ms at 60Hz** (≤7–8ms p99 at 120Hz); **mobile p50 ≤10–11ms, p99
≤14–15ms at 60Hz**, jank rate under 5% of frames (vs. Google's 25%
flag-threshold). This is a derived synthesis, not a single citable number —
flagged as such.

**Honest gap in the field, not just in Lumen:** rigorous frame-time
percentile data barely exists outside Google's own Compose benchmarks.
Qt/QML and GTK4 — two of the most-shipped native toolkits in the world —
have no public percentile data at all. A reviewer should not assume silence
means good performance; it means the instrument was never pointed at it in
public.

### 2.2 Scroll performance, large list

| Framework | Result | Evidence | Source |
|---|---|---|---|
| React-Virtualized (1M rows) | ~10s load, >3GB heap | MEASURED | [Render 1M rows in React](https://medium.com/@priyankadaida/how-to-render-a-million-rows-in-react-with-react-virtualized-for-high-performance-56733981c3ea) |
| Rust/WASM table (Table RS, 1M rows) | ~2s load, ~1.1GB heap | MEASURED | Community benchmark cited by the frame-time research pass |
| RevoGrid (400k rows) | Smooth, 720MB RAM | MEASURED | [Battle of the rows](https://dev.to/revolist/battle-of-the-rows-the-limits-of-data-performance-4mcb) |
| Flutter, Compose, SwiftUI, Qt/QML, GTK4 | **No published 1M-row benchmark for any of them** | — | Vendor guidance for all five stops at "use virtualization/lazy-loading"; none quotes a number at that scale |
| egui, Slint, Dioxus, Makepad | **No published benchmark at any scale** | — | — |

This is the most striking gap in the entire external research pass: **not
one of the eleven competitor frameworks has a public 1M-row scroll
benchmark**, native or otherwise. The only real data at that scale comes
from web/WASM table libraries, not from any GUI framework proper. This
directly bears on where Lumen could differentiate (§6).

### 2.3 Startup / time-to-first-frame

| Framework | Cold start | Evidence | Source |
|---|---|---|---|
| Rust-native, wgpu backend: **egui** | 200–300ms (Linux, eframe) | MEASURED | [Tauri/Iced/egui comparison](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html), 2023 |
| Rust-native: **iced** | 217–333ms (Linux) | MEASURED | Same source |
| Rust-native shell: **Tauri** | 380ms (2026), 366–417ms (2023, two independent measurements) | MEASURED | [Tauri vs Electron 2026](https://tech-insider.org/tauri-vs-electron-2026/); Kalbertodt 2023 |
| **GTK4** | ~230ms baseline; **+2–3s with libadwaita theming** (a documented regression, not the floor) | MEASURED | [GNOME GitLab #4361](https://gitlab.gnome.org/GNOME/gtk/-/issues/4361) |
| **Qt/QML** | No current benchmark found | — | Historical Qt4 data (2010) is too stale to use |
| **Flutter** (mobile) | ~2s target (vendor guidance); one 2026 comparison claims sub-200ms with Impeller (lower confidence, single source) | VENDOR-PUBLISHED / MEASURED (mixed confidence) | [Flutter startup optimization](https://medium.com/@reach.subhanu/optimizing-flutter-app-startup-cold-launch-to-ready-in-2-seconds-4ed32fa7a95f) |
| **Jetpack Compose** | TTID +2.5%, TTFD +13% vs. equivalent View-based screen (i.e. a *relative* cost of adopting Compose, not an absolute floor) | MEASURED, official | [Compose Hero Benchmarks](https://developer.android.com/develop/ui/compose/performance/herobenchmark) |
| **SwiftUI** (iOS) | ~400ms target | VENDOR-PUBLISHED | [App launch time](https://www.avanderlee.com/optimization/launch-time-performance-optimization/) |
| **Avalonia** | 2.4s baseline → 1.4s with `PublishReadyToRun`; Android 1,960ms → 460ms with NativeAOT | MEASURED | [Avalonia startup issue](https://github.com/AvaloniaUI/Avalonia/issues/5242); Avalonia 12 blog |
| **Electron** (floor, ceiling direction) | 1,420ms | MEASURED | Tech Insider 2026 |
| **LUMEN** | **250ms median** (`counter-win`, 3 runs: 332/250/175ms) | **MEASURED**, own bench | `docs/comparison-gtk-mintupdate.md §5` |

**Synthesized A+ bar for a Rust-native desktop app**: **<300ms**, on the
evidence of egui/iced/Tauri clustering in the 200–400ms band.

**Lumen's position: already inside the A+ band, on measured numbers on both
sides.** 250ms beats iced's 217–333ms range's midpoint, beats Tauri
(366–417ms / 380ms), and is competitive with egui's 200–300ms — despite
paying for a synchronous GPU-context creation that the performance review
(F23) flags as unoptimized headroom (font registration, first layout/paint,
and GPU pipeline compile are all serialized with no placeholder frame).
**This is the one numeric axis where Lumen needs no architectural change to
claim A+ — it may already be there**, modulo re-measuring on a larger real
app (all current numbers are for `counter-win`, a trivial app).

### 2.4 Binary size (real hello-world, release)

| Framework | Size | Evidence | Source |
|---|---|---|---|
| Native Android, no framework | 6KB–121KB | MEASURED | [Smallest Android app](https://ajinasokan.com/posts/smallest-app/) |
| **Slint** (release, Windows) | 3.5MB standard; 2.8MB with a11y+software-renderer disabled; ~23MB with the Skia renderer | MEASURED, maintainer-provided | [Slint discussion #9570](https://github.com/slint-ui/slint/discussions/9570) |
| **egui** (release, Windows, boilerplate) | 2.5MB; ~7.5MB with ~100 LOC and unstripped debug symbols | MEASURED | [egui discussion #1651](https://github.com/emilk/egui/discussions/1651) |
| **iced** (counter app, Windows, `opt-level="z"` + LTO) | 3.1MB | MEASURED | [iced discussion #1531](https://github.com/iced-rs/iced/discussions/1531) |
| **Dioxus** (desktop) | <3MB claimed | VENDOR-PUBLISHED | dioxuslabs.com |
| **Tauri** | 2.5–3.2MB | MEASURED | Tech Insider 2026; [Tauri app-size guide](https://v1.tauri.app/v1/guides/building/app-size/) |
| **GTK4** (dynamically linked) | No hard number found; estimated 500KB–2MB executable (excludes shared system libs) | ESTIMATED | — |
| **Flutter** (Android APK) | 8–10MB baseline, down to 4.7MB with split-per-ABI + obfuscation | MEASURED | [Why Flutter apps are big](https://www.javathinking.com/blog/flutter-apps-are-too-big-in-size/); [Flutter app-size docs](https://docs.flutter.dev/perf/app-size) |
| **Jetpack Compose** (delta over Views) | +782KB | MEASURED | [Compare Compose/View metrics](https://developer.android.com/develop/ui/compose/migrate/compare-metrics) |
| **Avalonia** (self-contained) | 60–80MB typical; ~26MB platform-specific optimized | MEASURED | [Trim Avalonia binary](https://github.com/AvaloniaUI/Avalonia/discussions/9217) |
| **Electron** | 45–115MB | MEASURED | Multiple independent sources |
| **LUMEN** | **22.1MB `hello` default; 34.5MB `counter-win`; 7.5MB lean profile (documented, not CI-verified)** | **MEASURED / DOCUMENTED-not-reproduced** | Resource-usage review rows 8–11 |

**Synthesized A+ bar for the Rust-native tier**: **2–5MB**, set by
egui/Slint/Tauri/iced all clustering there. **Lumen's own <5MB target
(`.ai_docs/01-architecture.md:70`) is exactly this bar — it's the right
target, correctly calibrated against its actual peer group.** Its lean
profile (7.5MB) is 1.5–3× over even the best Rust-competitor numbers, and its
default build (22.1MB) is 4–9× over, dominated by one file: a 15.5MB
CJK/RTL font shipped default-on (resource review F3). **This is a clean,
measured, fixable gap of degree** — no competitor here needed a different
architecture to hit 2–5MB; Lumen needs a feature-flag default flip and CI
coverage for the profile that already gets closest.

### 2.5 Memory — idle RSS and per-node cost

| Framework | Idle RSS (minimal windowed app) | Per-node/widget cost | Evidence | Source |
|---|---|---|---|---|
| **GTK4** (Rust + libadwaita) | ~30MB | Not published | MEASURED | [Toolkit memory footprint](https://szibele.com/memory-footprint-of-gui-toolkits/) |
| **Avalonia** | ~41MB | Not published | MEASURED | Avalonia 11.1 release notes |
| **Jetpack Compose** | 50–70MB; "30% smaller than equivalent XML" (relative claim) | Not published | MEASURED | Google reference apps |
| **SwiftUI** (iOS) | ~50MB | Not published | ANECDOTAL | Apple Developer Forums |
| **egui** | <20MB (no formal benchmark; extrapolated from immediate-mode's lack of retained state) | Not published (per-frame allocation, not per-node retention, by design) | ESTIMATED | — |
| **Slint** | Runtime overhead <300KiB is a vendor marketing figure; full app RSS not independently measured | Not published | VENDOR-PUBLISHED (partial) | slint.dev |
| **Qt/QML** | ~50–70MB estimated | Not published | ESTIMATED (weak) | Qt Forum thread |
| **Electron** (ceiling reference) | 100–150MB | Not published (web DOM, not applicable the same way) | MEASURED | — |
| **LUMEN** | **292MB `counter-win`, 270MB `datagrid-win`** (RSS); **69MB `Pss_Anon`** (the app's genuinely-own memory, vs. mintupdate's GTK3/Python 43.6MB — a 1.6× ratio, "normal, not scandalous") | **161B/node (Tree, SoA)**; **1008B/node (Element, rebuilt every view pass)**; **256B (LayoutStyle, inline)**; **160B (DrawCmd)** — **field-matched reconstruction, the only per-node byte cost published by any framework in this entire comparison set** | **MEASURED**, own bench + reconstruction | `docs/comparison-gtk-mintupdate.md §3`; resource-usage review rows 1–4 |

**Two things worth separating clearly.** First, Lumen's RSS headline (292MB)
is *worse than every competitor's number found, including Electron's
ceiling* — but ~123MB of that is GPU/shader-compiler driver residency
(`libLLVM.so`, NVIDIA's `libnvidia-gpucomp.so`, `/dev/nvidiactl` mappings),
not application or framework state, and it is specific to this box's NVIDIA
proprietary driver — a Mesa/Intel/AMD box would show materially less
(`docs/results-idle-and-gpu-context.md §2.2`). The *fair* comparison, `Pss_Anon`
(memory the process actually, uniquely owns), is 69MB vs. GTK3/Python's
43.6MB — a normal 1.6× gap, not the 3.3× the RSS headline implies. Second,
**Lumen is the only framework in this survey that publishes a real per-node
byte cost at all** — every competitor examined, including mature toolkits
like Qt and GTK4, has no public per-widget memory disclosure. That
transparency is itself a differentiator worth keeping even though the
underlying number (`Element` at 1008 bytes, driven by 11 always-allocated
`Option<Rc<dyn Fn>>` handler slots plus an inline 256-byte `LayoutStyle`,
resource review F4) needs to shrink.

### 2.6 Idle power / CPU

| Framework | Idle CPU | Evidence | Source |
|---|---|---|---|
| Native-event-loop toolkits generally (Qt, GTK, Avalonia, Cocoa/Win32) | Genuinely 0% at idle, by architecture | VENDOR-PUBLISHED / architectural | OS-native event loops correctly sleep |
| **GTK4** | 0% by design; **but** a documented NVIDIA-Vulkan-renderer background-CPU bug exists independent of GTK's own loop | MEASURED (bug report) | [NVIDIA forums, GTK4 Vulkan renderer background CPU](https://forums.developer.nvidia.com/t/560-35-03-gtk4-apps-background-cpu-usage-with-vulkan-renderer/311721), 2025 |
| **Electron** | 2–5% observed (non-zero; V8/Node scheduler background tasks), historically as high as 5% (Slack, 2017) | MEASURED | Multiple GitHub issues |
| **LUMEN** | **0.40% (`counter-win`) / 1.90% (`datagrid-win`, unexplained anomaly)** on NVIDIA proprietary driver; **0 jiffies (0.00%) on lavapipe/Mesa**, same binary | **MEASURED**, own investigation; event loop itself proven correct (`about_to_wait` called once in 12s) | `docs/results-idle-and-gpu-context.md §1` |

This is a genuine, striking cross-validation: **Lumen's own investigation
independently rediscovered the exact same class of bug the GTK4/NVIDIA
forum thread reports** — a proprietary Vulkan driver polling at ~100Hz
regardless of which GUI toolkit sits on top of it. This is strong evidence
the residual idle-CPU cost is a *driver* characteristic, not a Lumen
architecture defect, and it would very likely reproduce on GTK4 too under
the same driver. Lumen's own event loop is correctly proven `Wait`-based.

### 2.7 Summary table — numeric bars vs. Lumen

| Metric | A+ bar (Rust-native tier unless noted) | Lumen measured | Verdict |
|---|---|---|---|
| Cold start | <300ms | **250ms** | **Already A+** (on a trivial app; needs re-verification on a real app) |
| Binary size | 2–5MB | 22.1MB default / 7.5MB lean (undocumented in CI) | 4–9× over; gap of degree |
| Idle RSS | 20–40MB (CPU-rendered) / 50–70MB (GPU-context-holding) | 292MB (123MB is GPU-driver residency) | Worst-in-class headline; fair-comparison (`Pss_Anon`) figure is only 1.6× the GTK/Python floor |
| Idle CPU | 0% | 0.40–1.90% (root-caused to NVIDIA driver, not Lumen's loop) | Loop logic already A+; residual is environment-dependent |
| Frame time (headless, 500-node) | p99 ≤12–14ms desktop | 0.776ms full rebuild / **1.114ms "incremental" (slower than full!)** | Full-rebuild path is excellent; the flagship incremental path is a measured *regression* |
| 1M-row scroll | No competitor has published this at all | 1.15ms/frame headless (`vlist_1m_scroll`) | Potentially uncontested strength — see §6 |
| Per-node memory | No competitor publishes this | 161B (Tree) / 1008B (Element) | Uncontested transparency; underlying number needs work |

---

## 3. The A+ bar — qualitative

### 3.1 Hot reload / live development

| Framework | Mechanism | Latency | Scope | State survives? | Rust-AOT achievable? |
|---|---|---|---|---|---|
| **Flutter** | Dart JIT VM bytecode injection | ~300ms (small app) → 800ms+ (large app), broken into 5 measured steps | Function bodies, full widget-tree structure, styling/layout | Yes, except: enum↔class changes, generic-type changes, `main()`/`initState()`/static-var changes → requires full hot **restart** | **No — gap of kind.** Requires a JIT VM. |
| **Jetpack Compose** (Live Edit) | JVM bytecode "trampolines" + JVMTI-based on-device interpretation | Target <250ms | Kotlin function bodies, Composable UI structure | Partial — JVMTI invalidation doesn't guarantee survival across all refactors; Preview/emulator-focused, not a production mechanism | **No — gap of kind.** Requires JVM/ART bytecode instrumentation. |
| **SwiftUI** (Xcode Previews) | `@_dynamicReplacement(for:)` + JIT (Xcode 16+) | Not quantified publicly | Function bodies, computed properties, initializers, subscripts only — **not** stored-property or type-signature changes | Scoped to the function being replaced | **Partial gap of kind.** Needs JIT-time dynamic replacement infrastructure. |
| **Qt/QML** | QML files reinterpreted at runtime (`QQmlEngine::trimComponentCache()`); C++ backing untouched | Not quantified; near-instant (I/O + reparse bound) | Full QML/JS structural changes; **C++ changes need full rebuild** | Preserved for QML-side state | **Yes — gap of degree.** DSL + interpreter pattern, provably portable to Rust (Slint already does it). |
| **Makepad** | Live-editable design DSL, separate from Rust business logic | Marketed as "instant"; no public benchmark | Styling, layout, scripted behaviors in the DSL; **Rust logic still needs recompile** | DSL state preserved; Rust state does not survive a Rust recompile | **Yes — gap of degree**, for the DSL layer specifically (proves the pattern in Rust). |
| **Slint** | Interpreter-based live preview (`SLINT_LIVE_PREVIEW=1`) | Not quantified; near-instant | Full `.slint` DSL structural changes; properties/models/callbacks preserved; **Rust logic does not hot-reload** | Yes, for DSL-side state | **Yes — gap of degree, already shipped in Rust.** |
| **Dioxus** | RSX/markup hot reload (no recompile); experimental `--hotpatch` (0.7+) for Rust logic via dylib reload | Milliseconds for RSX; unquantified for `--hotpatch` | Markup/styling: full. Rust logic: experimental, dylib-based | Markup: yes. Rust logic: subject to the same AOT constraints below | Markup — gap of degree (shipped). Rust logic — **same gap of kind as Lumen's own Tier 2.** |
| **egui** | None built-in; relies on external tools (Trunk for wasm, `cargo-watch`) | N/A | N/A | N/A | **Gap of kind**, compounded by immediate-mode having no retained tree to patch in place. |

**Why Rust AOT cannot replicate the Flutter/Compose/SwiftUI tier (the
genuine gap of kind, sourced):** no stable Rust ABI across compiler
invocations (function layout, generic monomorphization, and struct layout
are not guaranteed stable between two builds of the same source);
aggressive LLVM whole-program optimization assumes code will never be
live-patched; dynamic-library unload/reload leaves dangling function
pointers that managed runtimes handle transparently and Rust does not;
static/thread-local layout can silently shift between builds. ([Robert
Krahn — Hot reloading Rust](https://robert.kra.hn/posts/hot-reloading-rust/);
corroborated independently by [Bevy's own hot-reload
struggles](https://github.com/bevyengine/bevy/issues/15613).) This is not a
Lumen-specific problem — it is the shared ceiling for every Rust GUI
framework surveyed, Dioxus's experimental `--hotpatch` included.

**A+ bar for a Rust AOT framework, honestly stated:** Tier 1 (declarative
DSL/stylesheet reinterpretation, matching Qt/QML, Slint, Makepad) is
achievable and should be the baseline; Tier 2 (arbitrary Rust logic,
state-preserving, sub-second) is **not** achievable at the Flutter/Compose
level by any Rust framework today, and any claim to have solved it should be
treated with the same skepticism this review applies to Lumen's own.

### 3.2 API ergonomics

Bar-setters, per the research pass:

- **SwiftUI (A+)**: property wrappers (`@State`, `@Binding`, `@Environment`,
  `@EnvironmentObject`) make reactive data flow syntactically explicit with
  minimal boilerplate; two-way binding is a first-class language feature,
  not a convention.
- **Jetpack Compose (A)**: compiler-plugin-driven "smart recomposition" —
  Composables are classified at compile time as Restartable/Skippable/
  Replaceable, and `@Stable` lets the compiler skip work automatically. The
  `Modifier` system is a fluent, chainable, third-party-extensible API for
  cross-cutting concerns (padding, click handling, custom drawing) without
  subclassing.
- **Flutter (A-)**: widget composition (small widgets nest into complex UIs)
  paired with the hot-reload feedback loop is an exceptionally tight
  edit-see cycle; state management (Provider-class patterns) needs external
  packages, which costs some of the ergonomic ground SwiftUI/Compose hold
  natively.

**Where Lumen already matches or beats this bar structurally:** no
`Message` enum, no `impl Application` trait, no generic-over-`Message`
widget tree — `App::new(impl Fn(&mut BuildCx) -> Element)` plus
`cx.signal(key, init)` removes exactly the boilerplate class (declare every
mutation as an enum variant, route through a central `update`) that costs
an LLM the most tokens-to-correctness in iced/Elm-style frameworks. Lumen's
own consumer-API review confirms this structural win is real (B- overall,
but explicitly "wins the structural comparison decisively" against iced).

**Where Lumen falls short of the bar:** ergonomics is not just about how
little you type to get something working — it's about whether a mistake is
caught. SwiftUI/Compose's property-wrapper and compiler-plugin machinery
catch classes of mistakes (mismatched types, missed dependencies) at compile
time that Lumen currently lets through as **silent runtime no-ops**: 39 of
89 `.lss` properties parse and do nothing, `cx.signal`'s key has no
compile-time link to its type, and handler-staleness protection
(`stable_handler!`) is opt-in rather than a bound on every widget
constructor. The consumer-API review catalogued 21 distinct silent-failure
modes — this is the actual API-ergonomics gap, not verbosity.

### 3.3 Modularity / extensibility

| Extension point | SwiftUI | Compose | Flutter | Qt | GTK4 | Lumen |
|---|---|---|---|---|---|---|
| Custom widget | Yes, via composition | Yes, Composable functions | Yes, `RenderObject` subclassing | Yes, `QWidget` subclassing | Yes, delegation | **Yes, `LeafWidget` trait** — genuinely open |
| Custom layout | Yes, custom containers | Yes, `Layout` composable | Yes, custom `RenderObject` | Yes, layout managers | Yes, layout delegation | Yes (taffy-backed) |
| Custom renderer backend | No (UIKit/AppKit only) | No (Skia-on-ART only) | No (Impeller/Skia only) | Raster/OpenGL, not pluggable | Cairo only | **Yes, `Renderer` trait, two real backends (TinySkia/Wgpu)** — ahead of every competitor examined |
| Custom style/theme property | Environment injection (fixed set) | Custom `Modifier`s (open) | Composition-based (open) | **`QStyle` plugin system — full visual-style replacement via `QStylePlugin`, real-world examples (Breeze, Fusion)** | CSS + theme managers | **No — `Style` is a closed struct, `apply()` a hardcoded `match`, no registration hook** |

**Qt's `QStyle` plugin system is the single clearest miss in this table.**
It is the deepest third-party style-extension mechanism among all
frameworks surveyed (covers painting, geometry, animation — not just a
token/color override), and it's exactly the axis Lumen's own modularity
review independently flagged as absent ("no equivalent seam for a
third-party style property... `Style::apply` is a closed `match`") despite
the architecture doc's stated "third-party widgets are first-class"
principle. On renderer and widget extensibility, Lumen already matches or
leads the field; on style-property extensibility, it's the one framework in
this table with **no** answer.

### 3.4 Architecture — what "complete agent observability" would have to beat

No competitor in this survey — Flutter, Compose, SwiftUI, Qt, GTK4, egui,
Slint, Dioxus, Makepad, Avalonia — has an equivalent of Lumen's
`Headless<R,E>` architecture: the same live structure that the renderer
paints from is what the agent/test harness queries, verified in Lumen's own
architecture review as true by construction, not by convention (`app.rs`'s
`Headless<R,E>` is the one implementation both `lumen-agent::handle()` and
`lumen-test::TestApp` call into). There is no external A+ bar to compare
against here because no competitor is built agent-observable from the
ground up — this is genuinely uncontested territory, not merely
best-in-class (see §6).

---

## 4. Gaps of degree vs. gaps of kind

The decisive classification, per the brief. A gap of **degree** closes with
optimization/engineering within the current architecture. A gap of **kind**
needs a different architecture.

| Gap | Classification | Reasoning |
|---|---|---|
| **Flagship incremental rebuild is 1.44× slower than a full rebuild, 85% more allocations** (`docs/results-node-cost-n0.md`) | **Degree.** | The SoA tree, dependency-tracking signal graph, and hashed identity scheme are all sound and bench-verified. The specific cost is `copy_node`'s per-node overhead (4 HashMap remove+insert pairs, a `LayoutStyle::clone()`, a fresh taffy node, per *copied* node) — a data-structure/implementation defect in one function, already diagnosed down to line ranges by the project's own CP-series plan. Nothing here requires abandoning the retained-tree model. |
| **GPU damage computed, then discarded on the live present path** (F1, performance review) | **Degree.** | The diff machinery (`culled_for_damage`, `damage_between`) already exists and is correct; `present_to_surface` simply never calls it. This is a one-function wiring bug, not an architectural limitation — the review's own top-ranked fix. |
| **No shipped list virtualization by default** (`Scrollable` is O(N)) | **Degree**, and arguably already solved. | `VirtualList` exists, works, and is fast (1.15ms/1M rows) — the gap is discoverability/defaults, not missing engineering. |
| **Binary size 4–9× over the Rust-native A+ bar** | **Degree.** | Driven by a default-on 15.5MB CJK font and default-on GTK3/D-Bus linkage — feature-flag and dependency-diet decisions, not architecture. Slint/egui/Tauri all prove 2–5MB is achievable in the same language and toolchain class. |
| **Idle RSS 292MB, GPU context forced even when the CPU renderer is selected** | **Mostly degree, one wrinkle of environment.** | The forced-second-wgpu-context-on-CPU-renderer bug is a genuine implementation inversion (`lumen-shell/src/lib.rs:491-497`) with a scoped fix (a `softbuffer`-backed `SoftPresenter`, ADR-003 escalation, not yet built) — degree. The *residual* driver-level idle-CPU cost of holding a Vulkan context on NVIDIA's proprietary driver (0.65% vs. 0.00% on Mesa/lavapipe) is outside Lumen's control on that specific driver stack — not fixable by any GUI framework's own code, only avoidable by not holding the context at all (which the softbuffer fix would achieve). |
| **Tier-2 code hot-reload (state-preserving Rust logic swap)** | **Gap of kind at the ceiling, gap of degree on top of it.** | No Rust AOT framework surveyed (Dioxus included) has closed the gap to Flutter/Compose/SwiftUI's JIT-backed seamlessness — that ceiling is architectural (no stable Rust ABI) and shared industry-wide. But *within* that ceiling, Lumen's current implementation doesn't even meet the achievable bar: its "ABI compatibility hash" is a hardcoded literal (`0x1111_2222_3333_4444`) matched against an equally hardcoded constant, not a real fingerprint of anything, and the only thing ever swapped across the FFI boundary in the working demo is a static C-string label — not a `build(cx) -> Element` closure. So there are two stacked problems: an unclosable-by-Rust ceiling, and a currently-unmet-even-so floor. |
| **`.lss` style-property extension point is closed** (vs. Qt's `QStyle` plugin system) | **Degree.** | Needs a `register_property(name, apply_fn)` hook threaded through the cascade evaluator — additive to existing data structures, not a rearchitecture, per the modularity review's own fix sketch. |
| **39/89 `.lss` properties silently parse and do nothing; signal key has no compile-time type link; handler-staleness protection is opt-in** | **Degree, and cheap.** | Every one of these is a diagnostic-emission or type-signature change (add a `Copy` bound, add a `PARSE_ONLY_PROPERTIES` allow-list with a test asserting completeness, enrich a panic message) — no data structure needs to change shape. |
| **Agent observability blind spots** (cascade-rejection reasoning, hit-test-miss reasoning, no structured tree-diff verb) | **Degree.** | All extend existing data structures (`computed_json_spanned`'s origin/span, `Tree::hit_test`'s internal candidate walk) that already carry the needed information — not new subsystems. |
| **No windowed/GPU-presented frame-time-percentile benchmark exists for Lumen at all** | **Degree** (it's a missing instrument, not a missing capability) — but blocking. | Until this exists, none of Lumen's own frame-time numbers can be compared to the p50/p99 bars in §2.1, because every current Lumen number is a headless CPU-pump bench, not a real windowed GPU-presented measurement. This is the single most important missing piece of self-knowledge before any competitive claim can be made credible. |

**Net read:** of the material numeric/architectural gaps found, **exactly
one** — Tier-2 hot-reload's ceiling — is a genuine gap of kind, and it is
shared by the entire Rust GUI ecosystem, not a Lumen-specific weakness.
Every other gap identified, including the one that currently falsifies the
"peak performance" pillar (the incremental-rebuild regression), is
architecturally closable without a rewrite. This is a materially better
position than "peak performance" being unattainable — it means the CP-series
plan's ordering (fix `copy_node` before anything else) is the right bet.

---

## 5. Where Lumen could plausibly be best-in-class

Not everywhere — the brief is right that a framework doesn't need to win
every axis. Four places stand out where the field is either uncontested or
where Lumen's own measured numbers, once the known bugs are fixed, would
plausibly lead:

1. **Agent observability / AI-driven testability — already uncontested.**
   No competitor examined targets this at all, let alone with a
   single-source-of-truth architecture proven by construction. This is the
   one dimension where "A+" isn't a number to chase, because there's no
   competing number. The work here is closing Lumen's *own* blind spots
   (§3.4), not catching up to anyone.

2. **1M-row list virtualization cost.** Not one of the eleven competitor
   frameworks surveyed has a public benchmark at this scale — the only
   comparable data is from web/WASM table libraries, which run in an
   entirely different (browser/GC) environment. Lumen's own
   `vlist_1m_scroll` at 1.15ms/frame (headless) is genuinely rare data. If
   promoted from "exists but easy to miss" to the framework's documented
   default for lists past ~100 items (the performance review's own
   recommendation), and re-measured on the live GPU-windowed path (not just
   headless), this could become a citable, defensible claim no competitor
   can currently contest — because none of them have put a number on the
   table to contest it with.

3. **Self-measurement transparency as a trust signal.** No competitor
   examined publishes a benchmark suite explicitly designed to falsify its
   own architecture's central claim, the way `docs/results-node-cost-n0.md`
   does ("the node-cost thesis is falsified"). For a framework whose primary
   user is an AI agent, this matters beyond marketing: an agent can be
   pointed at `docs/results-*.md` and ground-truth benchmarks instead of
   vendor claims, and get an honest answer. This is a process/culture
   differentiator, not a number, but it compounds with #4 below.

4. **Per-node memory transparency.** Lumen is the only framework in this
   survey with a published, reproducible per-node byte cost (161B tree node,
   1008B `Element`, via field-matched `size_of` reconstruction). The
   *current* number needs to shrink (1008B is inflated by 11
   always-allocated handler slots and an inline `LayoutStyle` — resource
   review F4), but the practice of measuring and publishing it at all is
   ahead of the field. Once the number itself is competitive, "here is
   exactly what a widget costs, measured, not estimated" becomes a real
   pitch no competitor can currently make.

Lumen should **not** try to claim best-in-class on binary size, idle RSS, or
frame-time percentiles against this comparison set any time soon — those are
real, measured, multi-times-over gaps against a field that includes
frameworks with a decade or more of optimization work behind them. The
credible strategy is: close the gaps of degree that are cheap (binary size
default flip, GPU-context-on-CPU-renderer bug, damage-to-GPU wiring), accept
Tier-2 hot-reload will not beat Flutter/Compose's ceiling, and lead loudly
on the four axes above where the field either isn't competing or where
Lumen's instruments are already better than anyone else's.

---

## 6. A costed competitive benchmark proposal

The review's own finding — that competitive benchmarking was cut as too
expensive, leaving "no Makepad, Slint, egui, or Flutter comparison anywhere
in the repo... the single most conspicuous gap in the competitive story" —
undersells how cheap the highest-signal version actually is. Broken into a
cheap 80% tier (runs today, on this box, no new hardware or toolchains
beyond two `cargo add`s and one `apt install`) and a full tier (adds
foreign toolchains, real mobile hardware, and CI enforcement).

### 6.1 Cheap tier — ~5 person-days, zero new hardware, zero disk-heavy toolchains

| # | Benchmark | What it requires | Why it's high-signal | Cost |
|---|---|---|---|---|
| 1 | **Lumen's own windowed, GPU-presented frame-time percentile bench** (p50/p95/p99 under sustained `Poll`-mode redraw, e.g. a running CSS transition or active scroll) | Nothing new — reuses the existing live-window agent driver plus a timestamp-diffed present hook | **Prerequisite for everything else.** Every Lumen number cited in §2.1/§2.7 today is a headless CPU-pump bench; none of them can be honestly compared to a competitor's real windowed number until this exists. This is also exactly the gap the performance review's own "what's missing entirely" section names first. | ~1–2 days |
| 2 | **egui (eframe) matched-workload comparison**: same 500-row list / 1M-row scroll shape as `nodecost.rs`, same box, same criterion methodology | `cargo add eframe`, pure Rust, no foreign toolchain | Directly answers the review's #1 flagged gap — a real *compiled Rust* competitor, not GTK3/Python. egui is the immediate-mode reference point Lumen's own performance review already invokes qualitatively ("egui's honest full-rebuild is very likely cheaper today than Lumen's incremental path") — this makes that claim numeric. | ~1 day |
| 3 | **Slint matched-workload comparison**, same shape | `cargo add slint`, pure Rust (interpreter or compiled mode), no foreign toolchain | Slint is one of only two frameworks Lumen's own design docs cite as the motivating architectural comparison (§1, Tier A) — and has never been measured. Closes the single most conspicuous gap named in the review verbatim. | ~1 day |
| 4 | **GTK4 (native, via `gtk4-rs`) matched-workload comparison** — replaces the GTK3/PyGObject comparison with a real compiled competitor | `apt install libgtk-4-dev` + `cargo add gtk4`, no new toolchain, system package only | Fixes the review's own explicit caveat that the existing GTK comparison is "not evidence against real compiled competitors" — same measurement methodology, same box, now apples-to-apples on language tier. | ~1 day |
| 5 | **Idle-RSS/idle-CPU/binary-size/startup harness extended to the three new competitors above**, reusing the existing `benches/gtkfloor.py`/`gtkrow.py`-style pattern | Nothing new — script extension | Turns items 2–4 from "frame time only" into full coverage of §2.3–2.6 against real Rust/C competitors, for the same one day of scripting effort already budgeted. | ~0.5 day |

**Total: ~5 person-days, on the existing Linux box, no new hardware.** This
alone would take Lumen from *zero* compiled-competitor benchmarks (the
review's stated single biggest gap) to three, covering exactly the
architectural peer group (egui, Slint) and the mature-toolkit floor (GTK4)
that matter most, plus — critically — Lumen's own first honest windowed
frame-time numbers.

### 6.2 Full tier — adds foreign toolchains, real mobile hardware, CI

| # | Addition | Requires | Why | Cost |
|---|---|---|---|---|
| 6 | **Flutter (Linux desktop, `flutter build linux --release`)**, same matched workload | Flutter SDK install (~1GB download, new toolchain, but runs on the existing Linux box) | Answers the "AOT heavyweight bound" question with a measured number instead of citing vendor blog claims — closes the review's other named gap. | ~2 days |
| 7 | **Real mid-range ARM Android phone** running the existing `nodecost` harness via `adb`/`cargo ndk` | One physical device (~$150–250) | The N0 doc's own mobile numbers are explicitly labeled "a floor, not an estimate" (x86_64-under-KVM emulator) and flag this as the one thing that needs a real device to settle — currently the single biggest unresolved uncertainty in Lumen's own mobile performance story. | ~1 day + hardware cost |
| 8 | **Qt/QML comparison**, same matched workload | Qt6 SDK install (~2–3GB, new toolchain) | The other mature retained-mode C++ competitor; answers a question GTK4 alone doesn't (a toolkit with a real mobile-shipping history and different layout-engine tradeoffs). | ~2 days |
| 9 | **iOS device/simulator numbers** — currently zero; no IPA has ever been built anywhere in this repo's history | A macOS runner (cloud CI Mac minutes or physical Apple hardware) — **does not exist in this environment today** | Most expensive line item, and the only one that can't be done on the current Linux box at all. Flagged as the honest limit of what "cheap" can cover. | ~3–5 days once the environment exists, plus recurring Mac CI cost |
| 10 | **CI-gate all of the above as ratio-based regression benchmarks** (not absolute nanoseconds — per the CP-series' own CP0 plan, which already scopes this) | Extends `scripts/perf_gate.sh`'s existing pattern | Without this, every comparison above rots the moment either side's code changes. This is one-time infra, not a benchmark itself. | ~1–2 days |

**Total full tier: ~10–15 person-days plus ~$150–250 one-time hardware,
plus recurring cost if/when Mac CI is added for iOS.** The honest
conclusion: **80% of the signal — closing the review's own stated biggest
gap (no Slint/egui/Makepad-class comparison) and producing Lumen's first
real windowed frame-time numbers — costs about a week and needs nothing the
project doesn't already have installed or one `apt`/`cargo add` away.** The
remaining 20% (Flutter, Qt, real ARM hardware, iOS) is real money and real
new toolchains, but is also the part that answers questions the project
cannot currently answer at all (mobile is measured only via an x86_64
emulator floor; iOS has never been built).

---

## 7. Sourcing and confidence — summary legend

- **MEASURED**: a specific, reproducible benchmark run with numbers, cited
  with URL and approximate date.
- **VENDOR-PUBLISHED**: official documentation, blog, or marketing claim —
  may be a target, not a measured result.
- **ESTIMATED/ANECDOTAL**: forum post, single impression, or extrapolation
  with no real instrument behind it — flagged explicitly wherever used, and
  never presented as equivalent to a measured number.
- **Lumen's own numbers** are further split: **MEASURED** (this review or a
  cited prior session ran the instrument), **DOCUMENTED-not-reproduced** (a
  number appears in Lumen's own docs/scripts but wasn't independently rerun
  for this study — e.g. the 7.5MB lean-profile figure), and **DISPUTED**
  (explicitly disclaimed by the source document itself — `docs/
  plan-node-cost.md`'s frame-phase breakdown, retired 2026-08-05 in favor of
  `docs/results-node-cost-n0.md`, which is what this study cites instead).

Full source list is inline per-claim above; the highest-value primary
sources reused across multiple sections were: [Android Vitals — render
performance](https://developer.android.com/topic/performance/vitals/render),
[Jetpack Compose Hero
Benchmarks](https://developer.android.com/develop/ui/compose/performance/herobenchmark),
the [Tauri/Iced/egui comparison](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html)
(Kalbertodt, 2023 — the single most-reused independent Rust-GUI benchmark
found), [Slint discussion #9570](https://github.com/slint-ui/slint/discussions/9570), [Robert Krahn —
Hot reloading Rust](https://robert.kra.hn/posts/hot-reloading-rust/), and
Lumen's own `docs/results-node-cost-n0.md`, `docs/comparison-gtk-mintupdate.md`,
and `docs/results-idle-and-gpu-context.md`.
