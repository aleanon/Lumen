# Goals scorecard v2 — post-remediation, 2026-07-20 (D9.4)

Companion to the (now historical) `review-goals-2026-07.md`. Same three
goals; this time every number is measured on this box today (commit range
`7580e74..8a042c2`, 71 remediation commits) by the CI-gated scripts, not
estimated.

## Goal 1 — peak performance

| Metric | Measured | Budget / gate | Mechanism |
|---|---|---|---|
| Cold start (exec → first painted frame, headless, min of 5) | **2.3 ms** | 300 ms (`cold_start_gate.sh`) | direct boot, no runtime init tax |
| Signal update (in-place, no JSON round-trip) | **~16 ns** (was 780 µs) | regression-tested | `Signal::update` in place |
| Changed-frame cost | **O(changed)** — `FrameStats.nodes_rebuilt` counts only touched spans; untouched spans copy forward | copy-forward test suite (A.3.2) | span memo + ctx-hash copy-forward |
| Idle cost | **0 frames** — event-driven loop; `about_to_wait` sleeps to the earliest deadline across all windows | R2 idle tests | damage-gated present |
| GPU frame | damage-culled display list, content-hash texture cache, tessellation cache | R.1/R.2 tests | `culled_for_damage` both backends |

## Goal 2 — minimal resource usage

| Metric | Measured | Budget / gate |
|---|---|---|
| RSS growth over 300 signal-write frames | **2.1 MB** | 32 MB leak gate (`cold_start_gate.sh`) |
| `hello` release binary (default profile) | **22.0 MB** (15.5 MB of it = the pan-unicode font, by policy) | ≤ 24 MB (`size_gate.sh`) |
| Lean-profile scaffold binary | **6.8 MB** | ≤ 8 MB (`size_gate.sh`) |
| wasm bundle (`hello_web`, release) | **23.1 MB** | ≤ 24 MB (`web_gate.sh`) |
| Framework HTTP client / async runtime | **none, ever** (ADR-M2; executor seam only) | dependency review |
| Telemetry | **none, by stance** (01 §9b) | — |

## Goal 3 — agent ability to verify applications in dev mode

- **Full loop, all live surfaces**: the agent drives the desktop window
  (TCP), the browser session (WebSocket relay → wasm dispatch), and the
  Android emulator (adb-scripted gate) — verified end-to-end this cycle
  (P.1/P.2/P.3). AT-SPI (screen-reader infrastructure) sees and *drives*
  the same tree (P.4: a real `doAction('click')` changed app state).
- **Verification vocabulary**: semantic tree + selectors, screenshots
  (whole-frame and zoomed-crop defect magnifier), reactive introspection
  (`getDeps`/`whatDependsOn`/`lastChange`), perf (`app.perf`), lint/audit
  (WCAG), session record → exported regression test.
- **Determinism**: virtual clock everywhere; golden tests byte-stable on
  the CPU renderer; fuzz-lite no-panic suites in every gate (E.3).
- **Flake rate observed this cycle**: 0 test flakes across ~15 full
  workspace gates (348–351 suites each). The two mid-cycle gate failures
  were environmental (disk-full; editing during a live gate) — both now
  ops rules, neither a test flake.

## Residual liabilities (tracked, not hidden)

- Formal security review (T7.3), semver-checked widget APIs + docgen
  (T7.2), per-window agent verbs (T5.2), WebGPU present + perceptual web
  goldens (T5.1), iOS simulator leg (needs macOS), signing/notarization
  (CI secrets), inspector keyframe editor (T6.4).
