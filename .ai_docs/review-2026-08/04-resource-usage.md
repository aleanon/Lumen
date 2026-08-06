# Resource-Usage Review — Lumen (2026-08-06)

Scope: memory, binary size, dependency weight, GPU/VRAM, threads/handles,
power/idle, mobile/web footprint, and dev-time build cost. Desktop = Linux/X11
(the only platform with a running build in this environment). No code was
modified; no `cargo build --workspace` or clean release build was run — disk
was 77 GB free with `target/` already at 70 GB. Where a real number was
needed, it came from either (a) artifacts already present in `target/`, (b)
building one small workspace member in isolation (`hello`, reusing cached
deps), or (c) a field-matched standalone crate outside the workspace that
reproduces a struct's exact field types and asks `rustc` for `size_of`. All
three are disclosed inline; nothing below is a guess dressed as a measurement.

---

## Verdict

**Desktop: C+.** The architecture underneath is sound — the event loop is
genuinely deadline-driven and idle CPU is a driver artifact, not a Lumen bug;
the SoA node tree is tight (161 B/node) with a real freelist; five of six
render-side caches (glyph atlas, shape/run/glyph caches, GPU image/tess
caches, shadow cache) are capped with real eviction. But the product misses
its own published budget by 4-7x out of the box: the default build embeds a
15.5 MB CJK/RTL font and links GTK3 + D-Bus on Linux, producing a 22-35 MB
binary against a documented "<5 MB hello-world" target that has apparently
never been met even by the lean profile (best measured: 7.5 MB). Three real
caches have no eviction at all (decoded-image cache, text-editor undo,
app-level undo `History<T>`), one of which is a `thread_local` that lives for
the process lifetime. An accessibility thread that a doc comment calls
"dormant… near-zero cost" is in fact spawned unconditionally on every Linux
window and opens a D-Bus connection nobody asked for. Multi-window support
allocates a full independent GPU device per window with no sharing and no
occlusion-driven teardown. None of this is exotic to fix — it's exactly the
kind of thing a resource audit exists to catch before it's someone's shipped
default.

**Mobile: D.** There is no memory-pressure handling at all — not stubbed,
not TODO'd, simply never wired to `MainEvent::LowMemory` on Android or to any
iOS memory-warning callback (the iOS `AppDelegate.m` doesn't even implement
the method). There is nothing to release even if it were wired: no
asset/image cache with a `clear()`, no atlas trim path reachable outside
tests. No APK or IPA has ever been built in this environment or, as far as
docs/backlog reveal, anywhere else recorded — mobile binary size is
completely unmeasured. The <800 ms mobile cold-start figure in the
architecture doc is explicitly flagged in the task graph as never measured
on real or emulated hardware. Android and iOS shells are CPU-rendered
(no wgpu at all in those crates), which sidesteps the desktop GPU-context
problem but says nothing about whether the 15.5 MB font and full ICU
segmentation data are excluded from a mobile build — by default they are not.

---

## Measured facts vs. estimates

Every row states exactly how the number was obtained. "Measured" = I ran the
command/read the byte count myself. "Field-matched reconstruction" = a
standalone scratch crate (outside the workspace, not touching project files)
declaring the *exact same field types* read from the real source, compiled
with `rustc` and asked for `size_of` — authoritative for layout, but
disclosed because it isn't literally the shipped type. "Documented" = a
number that appears in the project's own docs/scripts, which I did not
personally reproduce. "Estimate/inference" = reasoning from source without a
number behind it.

| # | Fact | Value | How obtained |
|---|---|---|---|
| 1 | `Tree` SoA per-node cost (11 parallel Vecs, high-water mark, never shrunk) | **161 bytes/node** | Field-matched reconstruction of `crates/lumen-core/src/tree.rs:50-68` (`Rect`=32, `Option<Rect>`=40, `Affine`=48, `NodeIndex`=8×3, etc.), compiled and measured via `mem::size_of` |
| 2 | `Element` (the per-node widget-description struct, rebuilt every view pass) | **1008 bytes** | Field-matched reconstruction of `crates/lumen-widgets/src/element.rs:134-259` (43 fields incl. 11×`Option<Rc<dyn Fn>>` handler slots at 16 B each = 176 B, inline `LayoutStyle`=256 B) |
| 3 | `LayoutStyle` (embedded inline in every `Element`, not boxed) | **256 bytes** | Same reconstruction, `crates/lumen-layout/src/style.rs:174-224` |
| 4 | `DrawCmd` (display-list draw command; one `Vec<DrawCmd>` entry per paint op) | **160 bytes** (every variant pays for the largest — `Rect{..brush:LinearGradient..}`) | Field-matched reconstruction of `crates/lumen-render/src/display_list.rs:278-363` |
| 5 | Bundled default font (`pan-unicode`, default-on feature) | **15,515,760 bytes** (14.8 MiB) | `ls -la crates/lumen-text/fonts/GoNotoKurrent-Regular.ttf` |
| 6 | Bundled lean font (Latin+symbols, `--no-default-features`) | **354,748 bytes** (346 KiB) | `ls -la crates/lumen-text/fonts/GoNotoKurrent-Latin.ttf` |
| 7 | Symbols supplement font (always embedded) | **171,732 bytes** | `ls -la crates/lumen-text/fonts/DejaVuSans-Symbols.ttf` |
| 8 | `hello` example, release, default features, stripped | **23,151,024 bytes** (22.1 MiB) | Measured: `cargo build -q -p hello --release` (reused cached deps from prior builds — not a clean/whole-workspace build), `ls -la target/release/hello` |
| 9 | `datagrid` bin, release, default features, stripped | **23,553,080 bytes** (22.5 MiB) | Measured: pre-existing artifact, `ls -la target/release/datagrid` |
| 10 | `counter-win` / `datagrid-win` examples, release, default features, stripped | **34,469,776 / 34,658,168 bytes** (32.9 / 33.1 MiB) | Measured: pre-existing artifacts in `target/release/examples/` |
| 11 | Lean `hello`-equivalent facade build | **7.5 MB** | **Documented, not reproduced by me** — comment in `scripts/size_gate.sh:5-6` ("Measured 7.5 MB at T.4"); corroborated by three independent doc citations the mobile/web sub-agent found (`.ai_docs/06-task-graph.md:166`, `docs/review-goals-2026-07.md:157`, `.ai_docs/07-decision-log.md:252`) all giving 22.0-22.1 MB default / 7.5 MB lean. I did not run the lean leg myself (it builds a second, fully separate cargo workspace from scratch — too costly under the disk constraint) |
| 12 | `<5 MB` hello-world target (01-architecture.md §9) | **Unmet** by both default (22.1 MB, ~4.4x over) and the lean profile (7.5 MB, ~1.5x over) | Measured (row 8) vs. documented target `.ai_docs/01-architecture.md:70` |
| 13 | Total unique crates in the `lumen` facade's normal dependency graph (default features: wgpu+snapshot+pan-unicode) | **314 unique packages**, 858 tree-print lines with duplication | Measured: `cargo tree -p lumen --offline -e normal --prefix none \| sort -u \| wc -l` |
| 14 | Total unique packages in the whole workspace's Cargo metadata (incl. dev/build deps, all 71 members) | **664 packages** | Measured: `cargo metadata --offline --format-version 1` piped through `json.load` and `len(d['packages'])` |
| 15 | `wgpu`'s own transitive subtree size (subset of row 13) | **97 packages** | Measured: `cargo tree -p wgpu --offline -e normal \| grep -c ...` |
| 16 | ICU (`icu_*`) crates pulled by `parley`'s `complex-scripts` feature | **20 crates**, ~30 MB combined *source* on disk (not compiled-binary size) | Measured: `cargo tree -p lumen --offline -e normal --prefix none \| grep -ic "^icu"`; `du -sh` on `~/.cargo/registry/src/*/icu_*` (`icu_segmenter_data-2.2.0`=12 MB, `icu_segmenter-2.2.0`=8.6 MB, `icu_properties_data`=2.9 MB, rest smaller) |
| 17 | Duplicate crate versions across the workspace lockfile | `syn` (v1, v2, **v3** — three copies), `toml_edit` (4 versions), `bitflags` (v1, v2), `thiserror`(+`-impl`) (v1, v2), `rustix` (v0.38, v1.1), `hashbrown` (v0.15, v0.17), `png` (v0.17, v0.18), `read-fonts`/`skrifa`/`font-types` (2 versions each), `getrandom` (v0.2, v0.3), `libloading` (v0.7, v0.8), `winnow` (3 versions), + others | Measured: `cargo tree --offline --duplicates -p lumen` |
| 18 | GTK3/glib/D-Bus/X11/Wayland are real dynamic links, not just Cargo-graph presence | Confirmed via `ldd`: `libgtk-3.so.0`, `libglib-2.0.so.0`, `libdbus-1.so.3`, `libX11.so.6`, `libwayland-{client,cursor,egl}.so` all present | Measured: `ldd target/release/examples/counter-win` |
| 19 | `image` crate codec provenance | `image = { version = "0.25", default-features = false, features = ["jpeg","gif","webp"] }` — these are pure-Rust decoders (no libjpeg/libwebp C linkage) | Measured: `Cargo.toml:180`, cross-checked no `*jpeg-turbo*`/`*libwebp*` in `ldd` output |
| 20 | Idle CPU on an idle `counter-win` (desktop, NVIDIA proprietary driver) | 0.40% wall CPU, traced to two driver threads in a 100 Hz `FUTEX_WAKE` loop; **0 jiffies** on lavapipe with the same binary | **Documented, not reproduced by me** — `docs/results-idle-and-gpu-context.md:1-30` (a prior session's strace/ICD-swap experiment). I independently verified the *code-level mechanism* it depends on (see Findings) but did not re-run the strace experiment myself |
| 21 | `datagrid-win`'s idle CPU (higher than counter, from the same source doc's own earlier measurement, `docs/comparison-gtk-mintupdate.md:127`) | **1.90%**, ~4.75x `counter-win`'s figure | **Documented, unexplained** — the idle-CPU re-investigation only re-tested `counter-win`; this number is never re-examined by the "correction" commit, and no code path was found (by source search) that would explain it |
| 22 | `target/` total size | **70 GB** (`debug`=68 GB, `release`=2.2 GB, android=752 MB, doc=15 MB) | Measured: `du -sh target`, `du -sh --max-depth=1 target/*` |
| 23 | `target/debug/deps` and `target/debug/incremental` | 48 GB (7,047 files) / 12 GB (1,801 subdirectories) | Measured: `du -sh`, `ls \| wc -l`, `find -maxdepth 1 -type d \| wc -l` |
| 24 | Workspace member count | **73 total** members in `Cargo.toml`, **51** under `examples/` | Measured: line-count within `members = [...]` |
| 25 | `CARGO_INCREMENTAL=0` set (2026-07-19, after target/ regrew to 151 GB once) is possibly not fully effective | 1,801 incremental subdirs exist and some postdate the mitigation commit, including artifacts from today's date | Measured (`find target/debug/incremental -newermt ...`), cause not confirmed — flagged as an open discrepancy |
| 26 | Existing Android build artifacts | `target/x86_64-linux-android/` = 752 MB of `.rlib`/`.so` only; **no `.apk` anywhere** | Measured: `find … -iname "*.apk"` → empty; `du -h` on the android target dir |
| 27 | Existing iOS build artifacts | **None** — no `.xcodeproj`, `.ipa`, or `.app` anywhere in the repo or `target/` | Measured: `find` for those extensions → empty |
| 28 | Existing wasm build artifacts | **None** — no `wasm32-unknown-unknown` directory under `target/` at all | Measured: `ls target/` shows only `debug/release/x86_64-linux-android/doc/criterion/flycheck0/tmp` |
| 29 | wasm CI size gate / last recorded measurement | Gate: ≤24 MB; last recorded run 22 MB (pan-unicode font dominates) | **Documented, not reproduced** — `scripts/web_gate.sh:14-16`, `.ai_docs/07-decision-log.md:427` |
| 30 | Main desktop app thread pool size | `std::thread::available_parallelism()` (fallback 4) — **CPU-count-scaled, unconditional** | Measured (source read): `crates/lumen-core/src/tasks.rs:257-282`, wired at `crates/lumen-shell/src/lib.rs:114` |
| 31 | AT-SPI/D-Bus accessibility thread | Spawned unconditionally on every Linux window via `accesskit_winit`/`accesskit_unix`, opens a D-Bus session-bus connection | Measured (source read, incl. the actual `accesskit_unix` dependency source): `crates/lumen-shell/src/lib.rs:469`; thread spawn at `accesskit_unix-0.13.1/src/context.rs:48` |
| 32 | Notify/inotify file-watcher fd usage | One inotify fd + N watch descriptors (not N fds); both call sites watch exactly one file, non-recursive | Measured (source read): `notify` 6.1.1's `inotify.rs:93-122`; call sites `crates/lumen-shell/src/lib.rs:158-160`, `crates/lumen-cli/src/dev.rs:27` |

---

## Scorecard

| Dimension | Rating | One-line reason |
|---|---|---|
| Memory footprint | **Weak** | SoA tree is genuinely tight (161 B/node) and 8 of 11 identified caches are properly capped — but the decoded-image cache and text-undo stack are unbounded, and `Element` at 1008 B/node (rebuilt every view pass) is 6x the tree's own per-node cost |
| Binary size | **Broken** | Default build (22-35 MB measured) is 4-7x over its own documented <5 MB target; the one lever the code comments call "the real size lever" (font subsetting) still leaves the lean build 1.5x over target, and that lean path is never exercised by the workspace's own CI (per the sibling modularity review) |
| Dependency weight | **Weak** | 314 unique crates for the default app graph, GTK3/D-Bus/X11 linked on Linux (contradicts the "pure Rust" framing), 4 copies of `toml_edit`, 3 of `syn`, 2 each of `bitflags`/`thiserror`/`rustix`/`hashbrown`/`png`/`read-fonts` |
| Idle power | **Adequate** | The control-flow logic is correct and verified (deadline-driven `Wait`/`WaitUntil`/`Poll`, gated `animate()` calls) — but the one data point that showed a real anomaly (`datagrid-win` at 4.75x `counter-win`'s idle draw) was never re-explained, and mobile idle power has never been measured at all |
| GPU/VRAM | **Weak** | Glyph atlas is properly capped with real overflow eviction, but each window (main + every secondary) gets a fully independent `wgpu::Instance`/`Adapter`/`Device` with no sharing, and none of it is torn down on minimize/occlude (no `Occluded`/`suspended` handling exists) |
| Threads & handles | **Adequate** | Every thread except two is correctly, narrowly gated (env var, Cargo feature, lazy-on-first-use); the CPU-scaled thread pool (up to 32 parked threads on a 32-core box) and the always-on AT-SPI/D-Bus thread that contradicts its own "dormant" doc comment are the two real misses |
| Build-time / dev cost | **Broken** | 70 GB `target/`, history of a prior 151 GB blowout, 51 example crates as workspace members, `CARGO_INCREMENTAL=0` traded away for disk safety (undermining the "fast iteration" pitch for anything short of tier-1 `.lss` hot reload), and tier-2 hot-reload's real-world latency (an actual `cargo build -p <crate>` subprocess) is never measured anywhere in the test suite |

---

## Findings

Numbered by severity, highest first. Each cites file:line, states the
resource cost, and gives a concrete fix.

### F1 — [Critical] Decoded-image cache has no eviction, no cap, and no clear API — a real per-process leak

`crates/lumen-widgets/src/asset.rs:17-19` (`thread_local! { static CACHE:
RefCell<HashMap<u64, RgbaImage>> }`) and `:125-127` (`ANIM_CACHE`). Every call
to `asset::png()`/`asset::decode()`/`asset::animation()` with a new content
hash inserts and nothing ever removes an entry — no cap constant, no LRU, no
public `clear()`. It's `thread_local`, so it lives for the UI thread's entire
process lifetime. Contrast: every other cache in the render/text stack (GPU
image/tess caches, glyph/shape/run caches, shadow cache) has an explicit `CAP`
constant with half-retention eviction — this is the one that got skipped.
**Cost:** unbounded — a session decoding a few hundred distinct images (user
uploads, generated QR/chart PNGs, per-item avatars keyed by URL) accumulates
hundreds of MB of RGBA pixel data with no way to reclaim it short of process
restart. **Fix:** apply the same `CAP` + half-retention pattern already used
by `gpu.rs`'s `img_cache`/`tess_cache` (`crates/lumen-render/src/gpu.rs:45-52,
2120-2126, 2437-2443`) — it's a known-good, already-implemented pattern one
file away.

### F2 — [Critical] Text-editor undo stack snapshots the whole buffer on every keystroke, unbounded

`crates/lumen-text/src/editor.rs:28-29` (`undo: Vec<Snapshot>, redo:
Vec<Snapshot>`), fed by `snapshot()` (`:93-100`, clones the entire text
buffer) called from `insert`/`backspace`/`delete` on every edit
(`:110-141`). No `MAX_UNDO`, no coalescing of adjacent inserts into one undo
step. This is the real engine behind `TextField`/`TextInput`/`RichDoc`
(`crates/lumen-widgets/src/text_field.rs:17`, `text_input.rs:3-5`,
`widgets_m4.rs:525,592`). **Cost:** O(keystrokes × document length) in the
worst case — a long typed document produces one full-length clone per
character, i.e. effectively quadratic memory growth in a single long editing
session. This is the actual "text-heavy app" memory bomb the review was
asked to look for — not the glyph atlas (which is fine). **Fix:** cap
`undo`/`redo` length (e.g. 100-200 entries) and coalesce same-kind edits
within a short time window (the standard editor-undo debounce pattern) before
pushing a new snapshot.

### F3 — [High] Default build embeds a 15.5 MB font and links GTK3+D-Bus: the "<5 MB hello-world" target is missed by 4-7x, and the "pure Rust" framing is contradicted on Linux

`crates/lumen-text/Cargo.toml:16-20` makes `pan-unicode` (the full 15.5 MB
`GoNotoKurrent-Regular.ttf`, `crates/lumen-text/src/lib.rs:31`) default-on;
`crates/lumen-widgets/Cargo.toml:12` forwards it. Measured: `hello` release
= 22.1 MB (row 8), `datagrid` = 22.5 MB (row 9), `counter-win`/`datagrid-win`
examples = 32.9-33.1 MB (row 10) — all against the documented `<5 MB`
target (`.ai_docs/01-architecture.md:70`). The lean profile
(`--no-default-features --features wgpu`) is documented at 7.5 MB
(`scripts/size_gate.sh:5-6`) — still 1.5x over target, and per the sibling
modularity review, that lean combination for `lumen-widgets` is "structurally
never built inside `cargo build --workspace`" — i.e. the one profile that
gets closest to the target is not CI-verified as a whole. Separately,
`ldd target/release/examples/counter-win` (row 18) shows real dynamic links
to `libgtk-3.so.0`, `libglib-2.0.so.0`, `libdbus-1.so.3` — a C toolkit and a
C IPC bus, pulled in by `rfd`/`muda`/`tray-icon`/`gtk` on Linux (confirmed in
`cargo tree -p lumen-shell -i gtk`). This is a legitimate, deliberate
trade-off for native file dialogs/tray/menus, but it means the "pure Rust"
claim is true of the language the framework's own code is written in, not of
its Linux runtime dependency closure. **Fix:** either (a) make the lean
profile the shipped default and require an explicit opt-in for the CJK/RTL
font (most apps are Latin-script-only; `App::font(bytes)` already exists as
the runtime escape hatch per the crate doc comment), and add lean-profile
whole-workspace CI coverage; or (b) if GTK-dependent native dialogs/tray are
kept, document the Linux runtime dependency honestly instead of "pure Rust"
without qualification.

### F4 — [High] `Element` costs 1008 bytes per node — 11 handler-pointer slots and a 256-byte inline `LayoutStyle` are paid by every element, whether used or not

Measured via field-matched reconstruction (row 2): `crates/lumen-widgets/src/
element.rs:134-259`. 11 `Option<Rc<dyn Fn(...)>>` fields (`on_click`,
`on_wheel`, `on_drag`, `on_drop`, `on_text`, `on_key`, `on_caret_set`,
`on_dismiss`, `on_increment`, `on_decrement`, `on_set_value`) cost 176 bytes
total even for a plain `<label>`-equivalent leaf that uses none of them; the
full `LayoutStyle` (256 B, row 3) is inlined rather than boxed/shared. This
is the struct analogue of the "big enum variant" antipattern the review was
asked to look for: one uniform, maximal payload shape for a heterogeneous
set of widget kinds. `Element` is `#[derive(Clone)]` and is the node type in
`children: Vec<Element>` — so a tree of N widgets costs at least N×1008
bytes just for the tree shape, before any per-node `Vec<String>`/`String`
heap payloads. For a data-heavy view (e.g. a 500-cell datagrid) that's
~500 KB just in `Element` stack shape, rebuilt on every view pass that
doesn't hit the scope-memo fast path. **Fix:** box the rarely-used handler
slots and `LayoutStyle` (an `Option<Box<Handlers>>` bundling all 11 pointers
would drop the common case to 8 bytes instead of 176), or move `LayoutStyle`
behind `Rc`/`Box` since it's already cloned wholesale on every element
clone.

### F5 — [High] Every window gets a fully independent `wgpu::Instance`/`Adapter`/`Device`/`Queue` — multi-window is N (or 2N) GPU contexts, not one shared context

`crates/lumen-render/src/gpu.rs:465-478` (`Wgpu::new()`) builds a fresh
Instance/Adapter/Device every call, with no shared/cached instance anywhere
in `lumen-render`/`lumen-shell`. `Shell::open_secondary`
(`crates/lumen-shell/src/lib.rs:877-925`) calls this for **every** secondary
window (`:879-880`), each also getting its own single-threaded executor
(`:884`). If an adapter can't present, that window *also* gets a second,
independent `Presenter` with its own Instance/Adapter/Device
(`:1435-1454`) — the same "CPU renderer forces a second GPU device" problem
`docs/results-idle-and-gpu-context.md` documents for the *main* window,
except this compounds per-window and isn't mentioned in that doc at all.
**Cost:** the idle-CPU doc measured ~123 MB of NVIDIA/LLVM driver residency
for *one* such context; `examples/multi_window` with N windows can hold up to
2N of them. **Fix:** hoist Instance/Adapter/Device/Queue creation to an
app-level singleton, share it across all window surfaces (standard wgpu
multi-window pattern — one device, N surfaces).

### F6 — [High] No GPU/surface teardown on minimize or backgrounding — desktop or mobile

Exhaustive enumeration of `WindowEvent::*` arms in
`crates/lumen-shell/src/lib.rs` found no `Occluded` handler, and
`ApplicationHandler::suspended` is never overridden (only `resumed`,
`:455`). The wgpu device/surface (and the fallback `Presenter`'s second
device, if present) stay fully resident when a window is minimized. On
Android, `MainEvent::LowMemory` (a real signal exposed by `android-activity`
0.6.1) is never matched — `crates/lumen-shell-android/src/imp.rs`'s event
match falls through to `_ => {}` for it. On iOS,
`crates/lumen-shell-ios/ios/AppDelegate.m` implements only
`didFinishLaunchingWithOptions:` — no `applicationDidReceiveMemoryWarning:`,
no `applicationDidEnterBackground:` method exists in the file at all.
**Cost:** on desktop, VRAM/driver resources for every open window persist
indefinitely regardless of visibility. On mobile, this is worse in kind: the
OS *will* kill backgrounded apps that don't respond to memory pressure, and
Lumen currently has no code path that would even know to respond — not a
missing call, a missing match arm. **Fix:** desktop — add an `Occluded`
handler that at minimum trims the glyph atlas and drops the readback buffer
(`crates/lumen-render/src/gpu.rs:1195-1213`) on full occlusion; mobile — wire
`MainEvent::LowMemory` and an iOS memory-warning delegate method to a shared
`on_memory_pressure()` that clears the (currently nonexistent, see F1) asset
cache and calls `GlyphAtlas::clear()`/`reset_glyph_cache()` (the latter is
presently `#[cfg(test)]`-only, `crates/lumen-text/src/lib.rs:118`, and would
need to become a real public API to be callable in production).

### F7 — [Medium] AT-SPI/D-Bus accessibility thread is spawned unconditionally on every Linux window despite a doc comment calling it "dormant… near-zero cost"

`crates/lumen-shell/src/lib.rs:296-299` documents the AccessKit adapter as
"Dormant (near-zero cost) until an assistive technology subscribes." But
`Shell::resumed` (`:469`) constructs `accesskit_winit::Adapter::
with_event_loop_proxy` unconditionally on every window open, and that
constructor reaches `accesskit_unix::context::get_or_init_messages()`
(`accesskit_unix-0.13.1/src/context.rs:39-58`), which spawns a background
thread that opens a **D-Bus session-bus connection** and runs an async
executor loop — verified by reading the actual dependency source, not
inferred. What's actually deferred is only AT-SPI object registration, not
the thread or the connection. **Cost:** one background thread + one D-Bus
connection per desktop Linux app launch, always, whether or not any
assistive technology is present — small in isolation, but it's exactly the
kind of gap between "documented as free" and "actually costs something" this
audit exists to catch, and D-Bus connection setup is not free on a cold
start budget that the same doc set targets at <300 ms. **Fix:** either
correct the doc comment to reflect reality, or make the adapter construction
itself lazy (defer to first AT-SPI probe) if upstream `accesskit_unix`
supports that.

### F8 — [Medium] App-level `History<T>` undo (the documented app-facing undo primitive) has no cap either

`crates/lumen-widgets/src/undo.rs:9-13` (`History<T> { past: Vec<T>, present:
T, future: Vec<T> }`), `push` (`:31-35`) unconditionally clones `present`
into `past` with no size check. This is the primitive the `building-apps`
skill documents for app-level undo, so any app that follows the documented
pattern for a large `T` (e.g. a whole-document state snapshot) and edits
frequently accumulates one full clone per edit for the session's lifetime.
**Fix:** same as F2 — cap history depth, document the cap in the skill/API
docs so app authors know it exists.

### F9 — [Medium] Session-recording step list in the live-window agent grows unboundedly unless the driving client explicitly resets it

`crates/lumen-agent/src/lib.rs:106-118` — `Session { steps: Vec<Step>,
recording: bool }`, `recording: true` by default, every dispatched action
appends (`:154-180`) with no cap; only reset by an explicit `session.start`
call (`:194`). Contrast: the adjacent diagnostic log ring in
`crates/lumen-core/src/state.rs:321-324,377-380` is properly capped at 1000
entries with `pop_front` eviction — this is the one sibling structure that
didn't get the same treatment. **Cost:** low per-entry (small strings) but
unbounded for a long automated agent-driven exploration/regression-generation
session, which is exactly this framework's primary use case (AI-agent-driven
UI). **Fix:** apply the same `VecDeque` + cap pattern used for the
diagnostic log.

### F10 — [Medium] `style_memo` cache has no cap between structural invalidation events

`crates/lumen-widgets/src/app.rs:610-613` — `style_memo: HashMap<u64,
Rc<StylePair>>`, only cleared wholesale on resize/theme/stylesheet-reload
(`app.rs:2105-2110, 2886, 2929`), unlike sibling caches (`img_cache`,
`tess_cache`, `shape_cache`, `run_cache`) which all apply half-retention
eviction under their own cap. Fine for a static class/id vocabulary; grows
unboundedly if an app author computes classes/ids from per-item unique data
(row index embedded in a class name, a timestamp, a UUID) — a pattern the
framework doesn't warn against. **Fix:** apply the same cap+half-retention
pattern, or add a lint/diagnostic for classes/ids that look procedurally
unique.

### F11 — [Medium] `hotpatch.rs`'s tier-2 dev-loop deliberately leaks every superseded dylib — correct for safety, but unbounded, and worth surfacing in dev-loop UX

`crates/lumen-cli/src/hotpatch.rs:36-41,82` — `HotComponent.retired:
Vec<Library>`; every hot-swap during a dev session pushes the old `Library`
handle and never calls `dlclose`, explicitly documented (`:1-9`) as a
deliberate choice to avoid a use-after-free (live pointers may still
reference the old code/rodata). This is the *correct* engineering call, not
a bug, but it means a long dev session doing many tier-2 swaps accumulates
one full loaded `.so` in memory per swap — worth a periodic-restart nudge in
the dev-loop UX (e.g. `lumen dev` warning after N swaps) since a developer
running an AI-agent-driven iterate-many-times-per-minute loop (this
framework's stated audience) will hit this faster than a human.

### F12 — [Medium] `target/` history: a documented prior 151 GB blowout, and the current disk-pressure mitigation (`CARGO_INCREMENTAL=0`) may not be fully effective

`.cargo/config.toml:1-15` — comments record "three target/-and-.git
corruption waves" and a day of dev cycles regrowing `target/` to 151 GB
before `CARGO_INCREMENTAL=0` was added (2026-07-19). Measured today:
`target/` = 70 GB, of which `target/debug/incremental` = 12 GB across 1,801
subdirectories, some dated after the mitigation commit and some as recent as
today (row 25). This is flagged, not confirmed, as a discrepancy — possible
explanations include a shell environment variable shadowing the `[env]`
setting (Cargo silently prefers an already-set env var unless `force = true`
is added), or these directories being pre-mitigation artifacts never
cleaned up. Either way, **`CARGO_INCREMENTAL=0` also means every full
recompile is a from-scratch compile for every crate touched** — a real
tension with "AI-first framework needs fast iteration," since it applies to
anything that isn't a tier-1 `.lss`-only change. **Fix:** verify whether the
`[env]` setting is actually taking effect (`cargo build -vv` and check for
`CARGO_INCREMENTAL` in the invoked rustc's env), add `force = true` if it
isn't, and separately evaluate `sccache` (not currently configured anywhere)
as a way to get incremental-like speed back without the disk/corruption
risk.

### F13 — [Low] `datagrid-win`'s idle-CPU anomaly (4.75x `counter-win`) was never re-explained by the correction that otherwise cleared Lumen

The idle-CPU investigation (`docs/results-idle-and-gpu-context.md`) is a
genuinely rigorous piece of work for the case it covers (`counter-win`,
0.40% → proven to be driver-side) — but the original comparison doc
(`docs/comparison-gtk-mintupdate.md:127`) also recorded `datagrid-win` at
1.90%, and the correction's own sentence changed from "neither app reaches a
true zero" to an unqualified "[Lumen] reaches [Wait]" without re-running the
`datagrid-win` case or explicitly narrowing the claim to `counter-win`. I
searched `examples/datagrid*` and its widgets for `cx.animate()` calls and
found none, so there's no static evidence datagrid polls — but "no static
evidence found" is not the same as "measured and cleared," and the doc
doesn't distinguish the two. **Fix:** re-run the same strace/ICD-swap
experiment against `datagrid-win` and either fold the result into the doc or
explicitly scope the "corrected" claim to the app that was actually tested.

---

## Unbounded-growth inventory

Every cache/buffer/collection surveyed across `lumen-core`, `lumen-render`,
`lumen-text`, `lumen-style`, `lumen-widgets`, `lumen-shell`, `lumen-agent`,
and `lumen-cli`, with its actual disposition. This is the most
safety-relevant section — treat "no cap found" entries as confirmed by grep
across the whole tree for eviction/cap logic, not by absence of a quick
look.

### Unbounded (no eviction, no reset path found)

| Structure | Location | Growth trigger | Severity |
|---|---|---|---|
| Decoded-image cache | `lumen-widgets/src/asset.rs:17-19` (`CACHE`), `:125-127` (`ANIM_CACHE`) — `thread_local` `HashMap` | Every distinct-content `asset::png()`/`decode()`/`animation()` call | **High** — process-lifetime leak, see F1 |
| Text-editor undo/redo | `lumen-text/src/editor.rs:28-29` | Every keystroke (`insert`/`backspace`/`delete`), full-buffer clone each time | **High** — worst-case quadratic in doc length, see F2 |
| App-level `History<T>` undo | `lumen-widgets/src/undo.rs:9-13` | Every `push()` call by app code following the documented undo pattern | **High if `T` is large**, see F8 |
| Agent live-window `Session.steps` | `lumen-agent/src/lib.rs:106-118` | Every dispatched agent action, unless client calls `session.start` | **Medium**, see F9 |
| `style_memo` (style-resolution cache) | `lumen-widgets/src/app.rs:610-613` | Every distinct `(id,classes,states,ty,ancestor-hash)` combination between resize/theme events | **Medium**, see F10 |
| `hash_to_id`/`id_to_key`/`scope_hash_to_id` (interned signal keys) | `lumen-core/src/state.rs:218-220`, grown by `intern_hashed` (`:885-893`) | Every never-before-seen interned signal/scope key; `evict_scope` (`:441-477`) frees slot storage but explicitly keeps this mapping ("cheap", `:436-438`) | **Medium** — fine for a stable key vocabulary, a leak if keys are per-event-unique (timestamps/UUIDs) |
| `HotComponent.retired` (dev-only) | `lumen-cli/src/hotpatch.rs:36-41,82` | Every tier-2 hot-swap during a `lumen dev` session | **Medium, dev-only, deliberate** — see F11 |

### High-water-mark growth (never shrinks after a peak, but doesn't accumulate garbage)

| Structure | Location | Note |
|---|---|---|
| `Tree`'s 11 SoA Vecs | `lumen-core/src/tree.rs:52-68` | Real freelist (`free: Vec<u32>`, `:54`) recycles slots correctly; `dealloc`/`alloc` (`:298-308`, `:266-296`) reuse before growing. But `shrink_to_fit` has zero call sites anywhere in `crates/` — a one-time spike (e.g. a huge list render) permanently costs 161 B/node × peak-count even after the tree empties back down. Low severity: this is capacity retention, not leaked garbage. |
| GPU readback buffer | `lumen-render/src/gpu.rs:75-78, 1195-1213` | Grows to the largest frame ever rendered, documented intentionally ("avoids a multi-MB allocation on every steady-state redraw"), never shrinks after a window is resized smaller. Low severity, single buffer. |

### Verified bounded — cited for completeness / audit trail

| Structure | Location | Cap / eviction mechanism |
|---|---|---|
| Glyph atlas | `lumen-render/src/atlas.rs:85-160`, `gpu.rs:309,897` | Fixed 1024×1024 single page; `alloc()` returns `None` on overflow (`atlas.rs:144-146`); GPU backend responds by `atlas.clear()` + full repack (`gpu.rs:1101-1103,1253-1256`), unit-tested (`atlas.rs:189-205`) |
| Shape cache / run cache | `lumen-text/src/lib.rs:235-237` | `SHAPE_CACHE_CAP=2048`, `RUN_CACHE_CAP=4096`, half-retention eviction (`:367-378, 416-427`) |
| Glyph coverage cache | `lumen-text/src/lib.rs:100-103` | `GLYPH_CACHE_CAP=8192`, capped (`:873-875`) |
| GPU image-bindgroup cache | `lumen-render/src/gpu.rs:45-46` | Cap 128, half-retention (`:2120-2126`) |
| GPU tessellation cache | `lumen-render/src/gpu.rs:50-52` | Cap 256, half-retention (`:2437-2443`) |
| Shadow-sprite cache | `lumen-widgets/src/app.rs:554` | Cap 64, half-retention (`:3771-3781`) |
| Canvas text-raster cache | `lumen-widgets/src/app.rs:549` | Cap 512, full-clear on overflow (`:4145-4149`) — inconsistent style (stall on crossing) but bounded |
| Diagnostic log ring (`Runtime::log`) | `lumen-core/src/state.rs:321-324, 373-385` | Hard cap 1000, `VecDeque::pop_front` |
| Per-node computed-style/layout-style caches | `lumen-widgets/src/app.rs:582,595,598-599` | Swapped via `mem::take` every rebuild (`:2498-2499`); bounded to live tree |
| Transition/animation state | `lumen-widgets/src/app.rs:619,623` | Swept every rebuild against the live id set (`:2651-2660`) |
| `scope_cache` (F5 view memoization) | `lumen-widgets/src/element.rs:594` | Swept every build, cleared on stylesheet/theme/resize (`app.rs:2106`) |
| Display list / draw-command buffer | `lumen-render/src/display_list.rs:367-369` | Fresh `DisplayList::new()` every frame (`app.rs:3557,3758`) — not accumulated |
| Input event queue | `lumen-core/src/events.rs:296-318` | Plain `VecDeque` drained every pump |
| Notify file-watcher | `lumen-shell/src/lib.rs:153-184` | Single-path, non-recursive; one inotify fd |

---

## Dependency weight table

| Subtree | Unique crates | What it buys | Concern |
|---|---|---|---|
| `wgpu` (full stack) | **97** (measured, `cargo tree -p wgpu`) | GPU rendering backend, desktop-only (target-gated off on wasm32, `lumen-render/Cargo.toml:20-27`) | Largest single subtree (~31% of the app's 314 unique crates); unavoidable for the GPU-accelerated desktop story, but note it's dead weight the moment a build targets wasm — the Cargo graph still resolves it for other targets in the same workspace, inflating `cargo check --workspace` time |
| `parley` + ICU (`complex-scripts`) | **20 `icu_*` crates**, ~30 MB combined source (`icu_segmenter_data`=12 MB alone) | CJK/Thai/etc. line-break/segmentation models, required or `parley` panics on non-Latin text (`Cargo.toml:120-122` comment) | Real functional need (documented: "without it parley panics… on CJK"), but bundled unconditionally — no feature gate to drop it for a Latin-only app the way the font has one |
| GTK3/glib/D-Bus | `gtk`, `gtk-sys`, `gtk3-macros`, `muda`, `rfd`, `tray-icon`, `libappindicator`, `zbus`+`zbus_macros`+`zbus_names`+`zvariant`+`zvariant_derive`, `accesskit_unix` | Native file dialogs, system tray, native menus, AT-SPI accessibility bridge on Linux | Directly contradicts an unqualified "pure Rust" claim on this platform (GTK3 is a C library); real, measured dynamic linkage (row 18) |
| `image` codecs | `image`, `jpeg-decoder`-class, `gif`, `image-webp` (feature-gated: `jpeg`,`gif`,`webp`, `default-features=false`) | jpeg/gif/webp decode for `asset::decode()` | Genuinely pure-Rust (verified: no `libjpeg`/`libwebp` in `ldd`); correctly optional (`codecs` feature, default-on but droppable) |
| Duplicated crate versions | `syn` (**3** copies: v1, v2, v3), `toml_edit` (4 versions), `bitflags` (v1+v2), `thiserror`+`-impl` (v1+v2), `rustix` (v0.38+v1.1), `hashbrown` (v0.15+v0.17), `png` (v0.17+v0.18), `read-fonts`/`skrifa`/`font-types` (2 versions each), `getrandom` (v0.2+v0.3), `libloading` (v0.7+v0.8), `winnow` (3 versions), `smol_str` (v0.2+v0.3) | N/A — pure waste | Each duplicate pair compiles twice and links twice; none of these are Lumen's own version pins (workspace pins one `syn`/`bitflags`/etc. per ADR-003) — they come from transitive deps disagreeing (likely `wgpu`'s stack vs. `accesskit`'s stack vs. build-tooling deps like `jsonschema`). Worth a `cargo update`/audit pass to see how many are resolvable |
| Whole graph | **314 unique** (normal deps, default features) / **664** (all packages incl. dev/build, all 71 members) | — | For comparison, a minimal `winit`+`wgpu` GPU app typically sits in the 150-250 unique-crate range; Lumen's 314 is elevated mainly by the GTK3/AT-SPI/ICU additions on top of that baseline, not by anything obviously gratuitous |

---

## Mobile readiness

What would actually happen under Android/iOS memory pressure today, based on
source (no device/emulator was used for this specific check, per the
instruction not to build):

1. **The OS sends the signal; Lumen doesn't hear it.** Android's
   `android-activity` crate already surfaces `MainEvent::LowMemory` — real,
   available, unused. `crates/lumen-shell-android/src/imp.rs`'s event match
   falls through to `_ => {}` for it. iOS's `AppDelegate.m` doesn't implement
   `applicationDidReceiveMemoryWarning:` at all — not a stub, absent.
2. **Even if wired, there's currently nothing productive to release on the
   render side**, because Android/iOS don't hold a GPU context in the first
   place (`lumen-shell-android` renders via the CPU reference renderer +
   `ANativeWindow::lock`; `lumen-shell-ios` hands back raw RGBA bytes for an
   external Metal template to blit) — so F6's "release the GPU surface" fix
   doesn't apply to mobile the way it does desktop.
3. **What *would* actually help on mobile is exactly F1 and F2** — the
   decoded-image cache and the text-undo stack are process-lifetime
   structures regardless of platform, and mobile is where a `LowMemory`
   signal existing-but-unheard turns "should eventually free memory" into
   "gets killed by the OS while the cache is still full."
4. **Font/ICU payload ships to mobile at full size by default.** Nothing in
   `crates/lumen-shell-android` or `-ios` opts into the lean font/no-ICU
   profile; an Android/iOS build inherits whatever features the app crate
   requests, and the workspace default is `pan-unicode` + full
   `complex-scripts` ICU data. For a platform where APK/IPA size is a
   documented app-store and cellular-download concern, this is the same
   4-7x-over-target problem as desktop, just with a stricter ceiling and,
   unlike desktop, currently zero measurement of the actual number (row 26,
   27 — no APK or IPA exists anywhere to measure).
5. **Cold-start**: the architecture doc's 800 ms mobile budget
   (`.ai_docs/01-architecture.md:70`) is explicitly flagged in the task
   graph as never measured on device or emulator (confirmed by the mobile/
   web sub-investigation) — an aspirational number, not a verified one.

**Bottom line:** a Lumen mobile app today would ship with desktop-scale
font/ICU payload, no way to hear the OS ask it to free memory, and (for
image-heavy screens) a growing cache that never shrinks even manually. None
of this has been measured on real hardware in this repository's history as
far as the docs record.

---

## Top 5 resource reductions, ranked by (bytes/watts saved ÷ effort)

1. **Cap the decoded-image cache (F1).** Effort: trivial — copy the
   `img_cache`/`tess_cache` half-retention pattern already implemented three
   times in `gpu.rs`. Payoff: closes the one confirmed unbounded-memory leak
   most likely to be hit by a real app (any app that decodes more than a
   handful of distinct images per session).

2. **Cap and coalesce the text-editor undo stack (F2).** Effort: small — add
   a `MAX_UNDO` constant and a "same edit kind within N ms" coalescing check
   in `editor.rs`'s `insert`/`backspace`/`delete`. Payoff: removes the only
   *quadratic* growth pattern found in the codebase, directly relevant to
   the framework's own text-heavy example apps (markdown editor, typed
   forms).

3. **Make the lean font/feature profile the shipped default, gate CJK/RTL
   behind an explicit opt-in.** Effort: medium — flip `pan-unicode` from
   default-on to default-off in `lumen-text`/`lumen-widgets`/`lumen`
   `Cargo.toml`s, document `App::font(bytes)` as the upgrade path (it
   already exists), add CI coverage for the now-default lean profile so it
   doesn't silently rot. Payoff: single largest binary-size lever available
   — drops the out-of-the-box binary from ~22 MB toward the documented
   ~7.5 MB lean figure (itself worth a follow-up dependency diet to actually
   hit <5 MB), directly benefits every platform including the two (mobile,
   wasm) where size is a hard distribution constraint, not just a nice-to-
   have.

4. **Share one `wgpu::Instance`/`Device` across all windows instead of one
   per window (F5).** Effort: medium — thread a shared device handle through
   `Shell::open_secondary` instead of calling `Wgpu::new()` per window; this
   is a standard, well-documented wgpu multi-window pattern. Payoff: cuts
   VRAM/driver residency roughly in half for any multi-window app (N devices
   → 1), and removes the compounding "second device for CPU fallback" cost
   per additional window.

5. **Wire `MainEvent::LowMemory` (Android) and a memory-warning delegate
   method (iOS) to a shared cache-clear callback (F6).** Effort: medium —
   the match arm and delegate method are currently simply missing, not
   broken; once F1's cache gets a real `clear()` (needed anyway) and
   `reset_glyph_cache()` is promoted out of `#[cfg(test)]`, this is mostly
   plumbing. Payoff: the difference between "eventually frees memory" and
   "responds to the one signal mobile OSes use before killing the process" —
   disproportionately valuable on mobile precisely because the framework
   currently has zero response to that signal today.
