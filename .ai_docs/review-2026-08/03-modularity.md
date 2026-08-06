# 03 — Modularity Review (independent, adversarial)

*Scope: crate boundaries, coupling/leakage, feature flags, platform seam, renderer
swappability, third-party extensibility, module-level structure, dependency
hygiene, examples-as-workspace-bloat, testability. Architecture-as-design is out
of scope (separate review). All claims verified against `Cargo.toml` files and
source, not against `.ai_docs/02–05` prose, per the review brief.*

---

## Verdict — **B-**

The top-level crate chain (`lumen-core → lumen-layout/lumen-render → lumen-text
→ lumen-style → lumen-widgets`) is genuinely layered: `cargo tree`-equivalent
inspection of every `Cargo.toml` found **zero cycles and zero upward
leakage** — `lumen-core` does not know about rendering, styling, or widgets,
and the Renderer trait (`crates/lumen-render/src/lib.rs:69`) has two real,
independently-swappable implementations (`TinySkia`, `Wgpu`) behind a
defaulted generic (`App<R = DefaultRenderer>`), not a privileged CPU path with
a bolted-on GPU afterthought. Dependency-version hygiene (ADR-003) is clean —
no crate declares a version directly outside the workspace root. That is the
good half.

The other half: **`lumen-widgets` (26k LOC) is not a widget library, it is
four different subsystems wearing one crate name** — the widget catalog, the
entire headless app runtime (`app.rs`, 4,613 lines, 18% of the crate),
an app-building toolkit (forms/nav/undo/i18n/system), and a11y/lint/audit
tooling — justified in its own module doc only by "it would create a
dependency cycle otherwise" (`crates/lumen-widgets/src/lib.rs:1-8`), not by
domain cohesion. Five widget files are named by *milestone* (`widgets_m1.rs`,
`widgets_m3.rs`, `widgets_m4.rs`, `widgets_extra.rs`, `misc_w2.rs`) rather than
by *domain*, and those milestone tags leak straight through the public facade
(`crates/lumen/src/lib.rs:36-39`) into third-party code as
`lumen::widgets_m3::DatePicker`. The project's own "lean build" feature
doctrine — the elaborate `default-features = false` + re-forwarding scheme
documented at `Cargo.toml:94-115` — is **structurally unverifiable by the
project's own CI**: `cargo build/test --workspace` (`.github/workflows/ci.yml:45-49`)
unifies Cargo features across all ~70 workspace members, so the lean paths in
`lumen-core`/`lumen-style`/`lumen-widgets` are *always* compiled with every
feature on in every CI run; the only place the lean profile is actually built
is a throwaway crate constructed *outside* the workspace by
`scripts/size_gate.sh:24-53`, and even that never runs `cargo test` against it.
And despite the "third-party widgets are first-class" principle (`.ai_docs/01-architecture.md:11`),
there is no equivalent seam for a third-party **style property** —
`Style` is a closed struct and `apply()` is a hardcoded string `match`
(`crates/lumen-style/src/style.rs:119,413-415`) with no registration hook.

None of this is fatal — the framework is pre-1.0, lockstep-versioned, and the
issues are mechanical (file reorganization, a CI job, an extension trait), not
architectural rewrites. But "modularity" graded on the mechanics specifically
asked about here — module boundaries doing what they claim, features doing
what their comments promise, extension points existing where the vision
document promises them — lands at a **B-**: a sound skeleton let down by an
overloaded crate, cosmetic-only file organization in the biggest crate, and a
feature matrix the CI cannot actually prove.

---

## Dependency graph

Built from every `Cargo.toml` under `crates/`, not from `.ai_docs`. Solid
edges = `[dependencies]`; dashed = target-gated or feature-gated;
dotted = `[dev-dependencies]`.

```mermaid
graph TD
    subgraph "Layer 0 — base"
        core[lumen-core]
        macros[lumen-macros]
    end
    subgraph "Layer 1 — engines"
        layout[lumen-layout]
        render[lumen-render]
    end
    subgraph "Layer 2 — content"
        text[lumen-text]
        style[lumen-style]
    end
    subgraph "Layer 3 — widget/runtime"
        widgets["lumen-widgets<br/>(catalog + App/Headless runtime<br/>+ app-building toolkit, one crate)"]
    end
    subgraph "Layer 4 — facade + platform"
        lumen[lumen facade]
        shell[lumen-shell — desktop]
        shellA[lumen-shell-android]
        shellI[lumen-shell-ios]
        shellW[lumen-shell-web]
    end
    subgraph "Layer 5 — tooling"
        agent[lumen-agent]
        cli[lumen-cli]
        test[lumen-test]
        smoke[skills-smoke]
    end

    layout --> core
    render --> core
    text --> core
    text --> render
    style --> core
    style --> layout
    widgets --> core
    widgets --> macros
    widgets --> render
    widgets --> layout
    widgets --> text
    widgets --> style

    lumen --> core
    lumen --> render
    lumen --> layout
    lumen --> text
    lumen --> widgets
    lumen -. "desktop-only cfg" .-> shell
    shell --> core
    shell --> render
    shell --> widgets

    agent --> core
    agent --> widgets

    shellA --> lumen
    shellA --> core
    shellA --> render
    shellI --> lumen
    shellI --> core
    shellI --> render
    shellW --> lumen
    shellW --> core
    shellW --> render
    shellW --> agent

    cli --> lumen
    cli --> core
    cli --> style
    cli --> widgets
    cli --> agent

    test -. dev/lib .-> lumen
    test --> core
    test --> macros
    test --> render
    test --> widgets

    smoke -. dev-deps only .-> core
    smoke -. dev-deps only .-> widgets
    smoke -. dev-deps only .-> test

    style2["46/51 examples bypass the facade<br/>(dep directly on core/render/layout/widgets)"]
    style2 -. "sanctioned by ADR-W2<br/>(.ai_docs/02-spec-core.md:317)" .-> core
```

**No cycles found.** No lower crate references a higher one (`lumen-core`,
`lumen-render`, `lumen-layout`, `lumen-style`, `lumen-text` were each grepped
for `lumen_widgets`/`lumen_shell`/etc. — zero hits). That is a real, verified
positive; the layering *diagram* in `.ai_docs/01-architecture.md §10` is
accurate on this point.

**What the graph doesn't show cleanly (the actual weak points aren't
cycles, they're parallel/redundant paths):**
- `lumen-shell-android`/`-ios`/`-web` each depend on **both** the `lumen`
  facade **and** `lumen-core`/`lumen-render` directly
  (`crates/lumen-shell-ios/Cargo.toml:332-335`,
  `crates/lumen-shell-web/Cargo.toml:353-356`), and their source imports
  `lumen_core::events::*` / `lumen_render::RgbaImage` directly rather than
  `lumen::events` / `lumen::render` — the facade re-export exists but isn't
  used even by the framework's own platform crates one layer up.
- `lumen-test` depends on the facade *and* on `lumen-core`, `lumen-macros`,
  `lumen-render`, `lumen-widgets` directly (`crates/lumen-test/Cargo.toml`) —
  defensible for a test/introspection harness, but it means the facade isn't
  a real chokepoint anywhere in the workspace except the four `hello*`
  examples and `hello`/`hello_web`'s dev-deps.

---

## Scorecard

| Area | Rating | One-line reason |
|---|---|---|
| 1. Crate boundaries / dep graph | **Adequate** | Cycle-free, correctly layered core→render/layout→text/style→widgets chain; undermined by `lumen-widgets` bundling four unrelated concerns. |
| 2. Coupling & leakage | **Adequate** | No lower-crate-knows-about-upper-crate violations found; but milestone-named modules (`widgets_m3`, etc.) leak raw through the facade, and `pub mod` visibility is used almost everywhere `pub(crate)` would do. |
| 3. Feature flags | **Weak** | The lean-build doctrine is well-commented and internally consistent on paper, but Cargo's feature unification under `cargo build/test --workspace` makes it *unverifiable* by the project's actual CI; only a build (not test) of a throwaway out-of-workspace crate ever exercises it. |
| 4. Platform modularity | **Adequate** | Three shells (android/ios/web) really do plug in without touching core — a genuine achievement — but they duplicate the same `render_into`/session glue with no shared code, and have already drifted out of feature parity (iOS has no key/wheel/agent-bridge support that web has). |
| 5. Renderer/backend swappability | **Strong** | Real trait, two real backends, already migrated to a defaulted generic (`App<R>`) with `Box<dyn Renderer>` as an explicit opt-in escape hatch, not the default — ahead of what project memory ("planned move") suggested. |
| 6. Extensibility for third parties | **Weak** | Widgets (`LeafWidget` trait) and renderers (`Renderer` trait) are genuinely open; `.lss` style properties are not — `Style::apply` is a closed `match` with no registration hook, despite the framework's stated "small stable core, everything else pluggable" principle. |
| 7. Module-level structure (big crates) | **Weak** | `lumen-widgets/src/app.rs` (4,613 lines) and `lumen-render/src/gpu.rs` (3,003 lines) are god-modules; five widget files are organized by ship-milestone, not by domain. |
| 8. Dependency hygiene (ADR-003) | **Strong** | Verified: no sub-crate `Cargo.toml` declares a version directly; every runtime/dev dependency is pinned once in the workspace root; `deny.toml` enforces license/advisory/wildcard policy consistent with ADR-020. |
| 9. Examples as modularity signal | **Weak** | 51 example crates are full workspace members, rebuilt/retested on every push across 3 OSes via `cargo build/test --workspace`; sanctioned by ADR-W2 (examples double as tests) but nothing separates a "framework-only" CI lane from the example fleet. |
| 10. Testability in isolation | **Adequate** | Each crate can be built/tested with `-p`; but the lean feature combination specifically is never tested in isolation anywhere in CI (see #3). |

---

## Crate-by-crate assessment

**`lumen-core`** (5.9k LOC) — Earns its existence cleanly: identity, tree,
SoA hot data, signals/state store, events, semantics, diagnostics — the one
crate everything else depends on, and it depends on nothing internal. Feature
gate (`snapshot`, `crates/lumen-core/Cargo.toml:129-130`) is the cleanest of
the three snapshot-gated crates. **Verdict: stays, no change.**

**`lumen-macros`** (445 LOC) — Forced to be a separate crate by
`proc-macro = true` (`crates/lumen-macros/Cargo.toml:175`); can't be merged
into anything. Small, single-purpose (`stable_handler!`, `text!`).
**Verdict: stays.**

**`lumen-layout`** (1.0k LOC) — A thin, disciplined Taffy wrapper; the ADR-004
promise ("no taffy type appears in its public API") is honored
(`crates/lumen-core/src/... ` grep found zero `taffy::` leakage outside this
crate). Small but earns its keep as the seam that would let Taffy be swapped.
**Verdict: stays.**

**`lumen-render`** (8.7k LOC) — Justified as the render-backend boundary
(display list, CPU + GPU backends, atlas, damage). The one internal problem is
`gpu.rs` at 3,003 lines (44% of the crate) — attach_surface, resize,
present, shader compile/render, and tessellation all in one file.
**Verdict: stays as a crate; split `gpu.rs` internally (surface/present vs.
shader vs. tessellation submodules).**

**`lumen-text`** (2.3k LOC) — Correctly scoped (parley/swash wrapper, IME,
editing) and correctly gated (`pan-unicode` feature,
`crates/lumen-text/Cargo.toml:440-441`) so a lean build can drop the 15 MB
font. **Verdict: stays.**

**`lumen-style`** (4.2k LOC) — Reasonably organized (lexer/parser/ast/style/anim
separated). The finding here isn't about crate existence, it's about the
*closed* extension surface — see Finding 5. **Verdict: stays; add an
extension seam (Finding 5).**

**`lumen-widgets`** (26k LOC, incl. 8.9k in `tests/`) — The one crate whose
boundary is wrong. It bundles: (a) the widget catalog proper (button, card,
grid, ~40 widget files, genuinely cohesive); (b) the entire headless
App/Headless runtime, checkpoint/restore, and animation engine
(`app.rs`, 4,613 lines); (c) an app-building toolkit (forms/nav/undo/i18n/
system/tasks, ~1.4k LOC) that is conceptually a layer *above* "widgets" (form
validation, OS clipboard/menu bridging, undo stacks); (d) accessibility/lint/
audit tooling (a11y/audit/wcag/design, ~0.5k LOC). The crate's own doc comment
admits the runtime lives here only to avoid a dependency cycle, not because it
belongs (`crates/lumen-widgets/src/lib.rs:1-8`). **Verdict: split — see
Finding 1 / Top-5 #3.**

**`lumen-shell`** (1.9k LOC) — Single 1,929-line `lib.rs` (no submodules) for
the whole desktop shell: winit event loop, clipboard, file dialogs, menus,
tray, a11y bridge, agent endpoint wiring. It's the only framework crate
*without* `#![warn(missing_docs)]` (checked: 0 hits vs. 1 in every sibling
crate's `lib.rs`), consistent with less internal discipline. **Verdict:
stays as a crate (desktop is legitimately a distinct platform surface); split
the file internally (winit loop / clipboard+dialogs+menu+tray / a11y bridge /
agent wiring).**

**`lumen-test`** (1.5k LOC) — A real Playwright-class harness
(locators/traces/sessions) that needs deep access to internals; its dependency
on both the facade and raw crates is the correct shape for a test harness, not
a leak. **Verdict: stays.**

**`skills-smoke`** (dev-only leaf) — One test per `.claude/skills/` entry,
pinning each skill's load-bearing snippet to the real API — directly
implements AGENT.md's doc-currency mandate. Tiny, single-purpose, dev-deps
only. **Verdict: stays; this is what "the 16th crate" is for.**

**`lumen-agent`** (2.0k LOC) — JSON-RPC/MCP dispatch over `Headless<R,E>`;
depends only on `lumen-core` + `lumen-widgets`, correctly excludes
`lumen-render` (doesn't need pixels, needs the semantic tree). The `ws`
feature (`crates/lumen-agent/Cargo.toml:24-26`) correctly isolates
`tungstenite` so `lumen-shell-web` can depend on it with `default-features =
false` (`crates/lumen-shell-web/Cargo.toml:362`) to keep `tungstenite→rand→
getrandom` out of the wasm build. **Verdict: stays; well-scoped.**

**`lumen-cli`** (2.1k LOC) — Dev server, hot reload orchestration, scaffolding,
distribution. Reasonable internal split (`dev.rs`/`hotpatch.rs`/`dist.rs`/
`agent.rs`). **Verdict: stays.**

**`lumen`** (57 LOC facade) — Exists, does its one job (re-export), but is
exercised by only `hello`, `hello_android`, `hello_ios`, `hello_web` among
~51 in-repo examples (5/51 by `grep`), and even the framework's own
android/ios/web shell crates reach past it for `lumen_core`/`lumen_render`
directly. Its re-export block for `widgets_extra`/`widgets_m1`/`widgets_m3`/
`widgets_m4` (`crates/lumen/src/lib.rs:36-39`) passes the internal
milestone-naming problem straight into the public API. **Verdict: stays (a
facade crate is the right shape); fix what it re-exports (Finding 3) and its
stale doc comment (Finding 4).**

**`lumen-shell-android`** (616 LOC) — The most complete of the three
non-desktop shells: owns its full event loop, IME (`KeyCharacterMap`),
safe-area/content-rect handling, tier-1 `.lss` hot reload, back-button→Escape
mapping (`crates/lumen-shell-android/src/imp.rs`). **Verdict: stays; the most
defensible of the three mobile/web shells as written.**

**`lumen-shell-ios`** (224 LOC) — Correctly split into a headless,
host-testable core (per its own doc comment) since iOS can only link on
macOS. But its session API is a strict subset of web's: touch + committed
text only, no key events, no wheel/scroll, no agent bridge
(`crates/lumen-shell-ios/src/lib.rs:113-136`). **Verdict: stays (forced by
the macOS-toolchain constraint); needs the same session surface as web
(Finding 6).**

**`lumen-shell-web`** (230 LOC) — The most complete of the FFI-thin shells:
pointer/text/key/wheel input, RAF-driven frame pump, agent JSON-RPC bridge
(`session_agent`, `crates/lumen-shell-web/src/lib.rs:219-230`). Its
`render_into` (lines 22-44) is near-byte-identical to iOS's
(`crates/lumen-shell-ios/src/lib.rs:30-52`) with no shared source.
**Verdict: stays; de-duplicate `render_into`/session boilerplate with iOS/
Android (Finding 6).**

---

## Feature-matrix analysis

| Crate | Feature | Default | Gates | Works? | Unification hazard |
|---|---|---|---|---|---|
| `lumen-core` | `snapshot` | on (`default-features=false` set by workspace root, `Cargo.toml:99`) | `serde_json` dep, `State: Serialize+DeserializeOwned` bound, `StateSnapshot` | Yes, compiles both ways per its own dev-deps forcing it on for tests (`crates/lumen-core/Cargo.toml:144-145`) | **Never isolation-tested off** — see Finding 2. |
| `lumen-style` | `snapshot` | on (root sets `default-features=false`, `Cargo.toml:106`) | `computed_json`/`canonical` JSON export | Presumed yes (mirrors lumen-core's pattern) | Same as above; not separately verified. |
| `lumen-text` | `pan-unicode` | on (root sets `default-features=false`, `Cargo.toml:105`) | embeds full 15 MB Noto Kurrent vs. ~350 KB Latin subset | Yes — `scripts/size_gate.sh` proves the lean binary boots and renders one frame | Only proven at the *facade* level via the out-of-workspace temp crate; `lumen-text --no-default-features` alone is never run in CI. |
| `lumen-widgets` | `wgpu`, `snapshot`, `pan-unicode`, `codecs` | all four on (`crates/lumen-widgets/Cargo.toml:466`) | GPU backend / JSON introspection surfaces / full font / jpeg+gif+webp decode | Compiles (implied by CI always building default) | **This is the crate every one of the 46 facade-bypassing examples depends on with `workspace = true` (full defaults) — see Finding 2; its lean combination is structurally never built inside `cargo build --workspace`.** |
| `lumen-shell` | `agent` | off | compiles `lumen-agent` + TCP JSON-RPC endpoint into the desktop binary | Yes, deliberately off-by-default per its doc comment (`Cargo.toml:300-302`) | None — additive, no forwarding trap. |
| `lumen-shell` | `wgpu`/`snapshot`/`pan-unicode` | off (forwarded, not chosen) | pass-through to `lumen-widgets` | Correct design (`Cargo.toml:303-307`: "nothing default here so the lean profile can drop them") | Same unification caveat as above — the facade always requests these on by default, so `lumen-shell`'s "off unless asked" stance is moot in the only build CI runs. |
| `lumen-shell-ios` | `snapshot` | on (`Cargo.toml:329-330`) | state-preserving rotate via snapshot handoff | Plausible; untested in CI (iOS never builds in `ci.yml` — only `mobile.yml`, and that's for Android per the earlier grep for `mobile`) | Not unification-affected (leaf crate) but also not CI-verified generally (needs macOS runner, out of this review's reach to confirm). |
| `lumen-agent` | `ws` | on | native `tungstenite` serve loop | Yes | `lumen-shell-web` correctly disables it (`Cargo.toml:362`) via a **direct path dependency**, explicitly to dodge the same unification trap the code comments call out for the facade (`crates/lumen-shell-web/Cargo.toml:358-362`) — this one is handled correctly, it's the pattern the widgets/core/style/text quartet *should* also get exercised against in CI. |
| `lumen` (facade) | `wgpu`, `snapshot`, `pan-unicode`, `agent` | first three on, `agent` off (`Cargo.toml:47-59`) | forwards to widgets/render/shell/text | Correctly wired forwarding chain (verified by reading every `[features]` block top to bottom) | This is the crate `scripts/size_gate.sh` builds *outside* the workspace specifically because in-workspace unification defeats it — the workaround is itself the proof of the hazard. |
| `examples/system_information` | `sysinfo` | off | optional richer host facts | Isolated, harmless (leaf example, no fan-out) | None. |

**Bottom line on the feature matrix:** the design (default-off low crates,
forwarded back on by consumers, `default-features = false` as *direct* path
deps where `workspace = true` would silently be ignored — correctly called
out in comments at `Cargo.toml:97-98`, `crates/lumen/src/lib.rs:66-69`
[reflected in the facade's `lumen-widgets` dep], `crates/lumen-shell/Cargo.toml:266-270`,
`crates/lumen-shell-web/Cargo.toml:358-362`) is coherent and the authors
clearly understand Cargo's feature-unification trap — they worked around it
correctly for the facade→widgets and shell-web→lumen-agent edges. **What's
missing is a CI job that actually isolation-tests the lean feature
combination for `lumen-core`/`lumen-style`/`lumen-widgets` themselves** (not
just builds a downstream smoke binary). Today nobody would notice if a
`#[cfg(not(feature = "snapshot"))]` code path in `lumen-core::state` bit-rotted.

---

## Findings (severity-ranked)

### 1. [High] `lumen-widgets` conflates the widget catalog with the app runtime engine
`crates/lumen-widgets/src/app.rs` is 4,613 lines — 18% of the crate's non-test
source — and contains `App`, `Headless`, `AppSnapshot`, `Checkpoint`,
`ReloadResult`, `NodeDeps`, `AnimVal`, `PropAnim`, `FrameStats`: the entire
headless runtime, checkpoint/restore protocol, and property-animation engine.
The crate's own module doc concedes the placement is circular-dependency
avoidance, not domain fit: *"It lives here, not in lumen-core, because it
depends on those higher crates"* (`crates/lumen-widgets/src/lib.rs:1-8`).
This is the concrete reason "should lumen-widgets be split" has a yes answer.
**Restructuring:** extract `app.rs`'s runtime types into a new `lumen-app`
crate sitting between `lumen-widgets` and `lumen` in the dependency chain
(depends on core/render/layout/text/style/widgets; the facade depends on it
instead of reaching into `lumen_widgets::app`). `lumen-widgets` re-exports
`lumen_app::{App, Headless, ...}` for one release to stay source-compatible.

### 2. [High] The lean-build feature doctrine is unverifiable by the project's own CI
`Cargo.toml:94-98` documents, correctly, that `lumen-core`/`lumen-style` default
to no features so `--no-default-features` builds actually drop `serde_json`,
and that `lumen-widgets` forwards `snapshot` back on by default. But CI's only
jobs are `cargo build --workspace --all-targets` / `cargo test --workspace`
(`.github/workflows/ci.yml:45,47,49`), and virtually every one of the 46
facade-bypassing example crates requests `lumen-widgets = { workspace = true }`
— i.e. full default features — in the *same* build graph. Cargo's feature
unification means `lumen-core`/`lumen-style`/`lumen-widgets` are compiled with
every feature on in every CI run; the `snapshot`-off / `pan-unicode`-off code
paths are never exercised. The only place the lean profile is built at all is
`scripts/size_gate.sh:16-53`, which constructs a throwaway crate in `mktemp -d`
*outside* the workspace specifically to escape unification — and even that
only runs `cargo build`, never `cargo test`, so lean-mode logic bugs (not just
size regressions) would go uncaught.
**Restructuring:** add a CI step that runs `cargo test -p lumen-core -p
lumen-style -p lumen-widgets --no-default-features` — each as a lone `-p`
target so it isn't pulled into the workspace-wide unification — verifying the
doctrine's actual claim, not just the shipped binary's size.

### 3. [Medium-High] Milestone-named files leak through the public facade
`crates/lumen-widgets/src/{widgets_m1,widgets_m3,widgets_m4,widgets_extra,misc_w2}.rs`
group widgets by *when they were built*, not *what they are*: `Modal`,
`PaneGrid`, `Select`, `Tooltip`, `Menu`, `Wrap`, `SplitPane` all share
`widgets_extra.rs` (`crates/lumen-widgets/src/widgets_extra.rs:65,135,247,322,359,431`)
with no thematic link; `DatePicker`/`TimePicker`/`AppBar`/`BottomNav`/
`PullToRefresh`/`NavigationRail` — pickers, navigation chrome, and a gesture
widget — share `widgets_m3.rs`. These names then leak, unrenamed, through the
facade: `crates/lumen/src/lib.rs:36-39` re-exports `widgets_extra, widgets_m1,
widgets_m3, widgets_m4` verbatim, so a third party's stable dependency surface
includes `lumen::widgets_m3::DatePicker` — an implementation-history artifact
baked into the public contract.
**Restructuring:** regroup by domain (`overlay.rs`: Modal/Tooltip/Popover/Menu;
`pickers.rs`: DatePicker/TimePicker/PickList/Combobox; `nav_chrome.rs`:
AppBar/BottomNav/NavigationRail; `panes.rs`: PaneGrid/SplitPane/Wrap/Grid),
then re-export flat from `lib.rs` so no milestone tag reaches the facade. Cheap
now (pre-1.0, lockstep `0.x`); expensive later.

### 4. [Medium] Facade doc comment overstates its own rule; facade is barely dogfooded
`crates/lumen/src/lib.rs:3-4` states *"User code and examples depend only on
`lumen`... nothing imports the internal crates directly (02 §11)"* — but
`.ai_docs/02-spec-core.md:317` itself carves out an explicit exception:
*"in-repo examples may depend on the internal crates directly (they double as
framework tests)"* (ADR-W2). The facade's own doc comment doesn't mention this,
which is a doc-currency gap under AGENT.md's binding rule. Substantively: only
5 of 51 example crates (`grep -l '^lumen = { workspace = true }' examples/*/Cargo.toml`
→ `hello`, `hello_android`, `hello_ios`, `hello_web`, `settings_android`)
actually exercise the facade; the other 46 depend on `lumen-core`/`lumen-render`/
`lumen-widgets`/`lumen-layout` directly. Even the framework's own platform
shells reach past the facade for types it already re-exports
(`lumen_core::events::*` instead of `lumen::events::*` in
`crates/lumen-shell-ios/src/lib.rs:22-24`, `crates/lumen-shell-web/src/lib.rs:15-17`).
**Restructuring:** fix the doc comment to state the ADR-W2 exception; migrate
a representative handful of examples (one per widget family, e.g. `counter`,
`todos`, `widget_gallery`) to depend on `lumen` only, so the facade's actual
public-API shape gets exercised by more than the four `hello*` toys.

### 5. [Medium] No extension point for a third-party `.lss` style property
`Style` is a closed, fixed-field struct (`crates/lumen-style/src/style.rs:117-119`)
and `apply()` is a hardcoded `match property { "display" => ..., "width" =>
..., ... }` over string literals (`crates/lumen-style/src/style.rs:412-420`)
with no fallback/registration hook — contrast with `LeafWidget`
(`crates/lumen-widgets/src/element.rs:91`), a genuinely public trait a third
party can implement today. The review brief's item 6 ("can someone... write a
style property without forking?") has a concrete **no** for this one
extension category, at odds with the architecture doc's "small stable core...
third-party widgets are first-class" principle (`.ai_docs/01-architecture.md:11`),
which is silent on style properties specifically.
**Restructuring:** either add a `register_property(name, apply_fn)` hook
threaded through `Stylesheet::compile`/`apply`, or explicitly document `@tokens`
as the supported (and only) third-party styling extension seam so expectations
are set correctly.

### 6. [Medium] Non-desktop shells duplicate session glue with no shared crate, and have already drifted in feature parity
`render_into()` is reimplemented near-verbatim in `lumen-shell-ios`
(`crates/lumen-shell-ios/src/lib.rs:30-52`) and `lumen-shell-web`
(`crates/lumen-shell-web/src/lib.rs:22-44`) — identical body (`App::new` →
optional stylesheet → `run_headless` → `pump` → `screenshot` → copy into an
output buffer), different doc comment. Feature coverage has already diverged:
web's persistent session supports pointer, text, **key**, **wheel**, and an
**agent JSON-RPC bridge** (`crates/lumen-shell-web/src/lib.rs:98-230`); iOS's
session supports only touch and committed text
(`crates/lumen-shell-ios/src/lib.rs:113-136`) — no keyboard, no scroll, no
agent bridge at all. `lumen-shell-android` (the most complete of the three,
`crates/lumen-shell-android/src/imp.rs`) shares zero code with either. Nothing
in the build enforces that a new platform (or a change to an existing one)
keeps the three in sync.
**Restructuring:** factor the common boot/pump/screenshot-to-buffer logic into
a shared internal module (or a small `lumen-shell-core` crate) all three call,
and treat the event-type checklist (pointer/text/key/wheel/agent-bridge) as a
compiled or at least doc-tracked contract each platform crate must satisfy.

### 7. [Low] Examples-as-workspace-members inflate every CI run, with no framework-only fast lane
51 example crates (10,723 LOC, `find examples -name '*.rs' | xargs wc -l`) are
full workspace members, so `cargo build --workspace --all-targets` / `cargo
test --workspace` / `cargo test --workspace --doc`
(`.github/workflows/ci.yml:45,47,49`) rebuild and retest all of them, three
times (ubuntu/windows/macos matrix), on every push — including pushes that
touch only `lumen-core`. This is partly intentional (ADR-W2: examples double
as regression tests) and the workspace has no `default-members` restricting
plain `cargo build`/`check` either, so the bloat is already the path of least
resistance everywhere, not just CI.
**Restructuring:** either split CI into a fast "framework" job (`-p lumen-core
-p lumen-render -p ... ` the 16 real crates) that runs on every push and a
slower "examples" job gated to changes under `examples/` or a framework crate,
or (the review brief's suggestion) hoist `examples/` into its own workspace
with a path-dependency back into this one — accepting the loss of true
single-command example regression testing that ADR-W2 currently trades for.

---

## Extension-point inventory

| Extension | Can a third party do this without forking? | Evidence |
|---|---|---|
| Write a custom widget | **Yes** | `pub trait LeafWidget` (`crates/lumen-widgets/src/element.rs:91`) — `measure`/`paint`/`semantics`/`event`, all public, documented. |
| Write a custom renderer backend | **Yes** | `pub trait Renderer` (`crates/lumen-render/src/lib.rs:69`), object-safe (`impl<R: Renderer + ?Sized> Renderer for Box<R>`, `lib.rs:194`), `App<R>` generic accepts it via `with_renderer(...)`. |
| Write a new platform shell | **Yes, by convention — no enforced trait** | Three independent implementations exist (android/ios/web) built only from `App`/`Headless`/`BuildCx`/`Element` public API; but there is no `Shell` trait, so nothing type-checks a new platform against the same contract (see Finding 6) — a fourth shell could silently omit capabilities the way iOS already has. |
| Add a custom `.lss` style property | **No** | `Style` is closed, `apply()` is a closed `match` (`crates/lumen-style/src/style.rs:119,413-420`) — Finding 5. Missing: a `register_property` hook or equivalent. |
| Register a custom stored-state trait object | **Yes, via a documented macro** | `#[lumen_macros::state_registry]` (`crates/lumen-core/src/registry.rs:1-28`) — an explicit, working extension point for ADR-013-compliant trait objects in the state store. |
| Add a new accessible `Role` | **No, closed enum, by design** | `pub enum Role` (`crates/lumen-core/src/semantics.rs:19`) is exhaustively matched in `role_to_accesskit` (`crates/lumen-widgets/src/a11y.rs:13`) specifically so a missing mapping fails to compile — reasonable given it mirrors a fixed platform-AT vocabulary (ARIA-like), not a gap worth fixing. |
| Depend on only the stable public API | **Yes for `lumen new` scaffolds, not for in-repo code** | `lumen-cli`'s scaffold template emits `lumen = { path/version }` only (`crates/lumen-cli/src/main.rs:377,384`) — correct; but in-repo dogfooding of that same constraint is thin (Finding 4). |

---

## Top 5 restructurings, ranked by (modularity gained ÷ churn cost)

1. **Add a real lean-feature CI job** (Finding 2). Cost: a few lines of CI
   YAML (`cargo test -p lumen-core -p lumen-style -p lumen-widgets
   --no-default-features`), zero source changes. Gain: turns the project's
   flagship feature-flag doctrine from "documented and hoped" into "CI-proven,"
   and would have caught any future `#[cfg(feature = "snapshot")]` regression.
   Highest ratio in this review by a wide margin.

2. **Regroup the milestone-named widget files** (Finding 3). Cost: pure file
   reorganization + `use` path updates + one facade re-export block change;
   no logic changes, mechanical enough to script. Gain: fixes the one place
   an internal implementation-history detail (`widgets_m3`) is permanently
   baked into third-party call sites — cheapest to do now, pre-1.0, most
   expensive to do after 1.0 freeze.

3. **Extract `app.rs`'s runtime into a `lumen-app` crate** (Finding 1). Cost:
   moderate — a new crate, a re-export shim in `lumen-widgets` for one release,
   and the facade's `pub use lumen_widgets::{app::FrameStats, App, ...}`
   (`crates/lumen/src/lib.rs:25`) becomes `pub use lumen_app::...`. Gain: the
   single highest-leverage fix to the "should lumen-widgets be split" question
   — cuts the crate by ~18% and gives the runtime its own versioned boundary
   distinct from the widget catalog.

4. **Add a `.lss` custom-property registration hook** (Finding 5). Cost:
   medium — touches the parser/cascade's closed `match` in
   `crates/lumen-style/src/style.rs:413-420` and needs a coherent story for
   how a registered property interacts with `Style`'s fixed fields (likely an
   auxiliary side-table rather than widening `Style` itself). Gain: closes the
   one clearly-absent extension point the "first-class third-party" principle
   promises but doesn't deliver for styling.

5. **Share the shell session/render-loop core across android/ios/web**
   (Finding 6). Cost: highest of the five — touches three platform crates,
   two of which (ios/android) need cross-compiled or emulator verification to
   confirm no regression, and closing the ios/web feature gap is new work, not
   just a refactor. Gain: real, but back-loaded — prevents *future* divergence
   more than it fixes anything broken today, since the current gap (iOS
   missing key/wheel/agent-bridge) hasn't yet been reported as a user-facing
   defect in this review's source material.
