# 05 — `lumen-test` Specification (normative)

Playwright-class testing for Lumen apps. Runs the real app headless on the CPU reference renderer by default (no GPU, no display server — CI-safe), or headed/GPU with a flag. Tests are ordinary `cargo test` integration tests.

## 1. Entry points

```rust
#[lumen_test::test]                  // headless, 800×600 @1x, light theme
async fn checkout_flow(mut app: TestApp) { … }

#[lumen_test::test(size(390, 844), scale(3.0), theme(dark), platform(ios_sim))]
async fn mobile_checkout(mut app: TestApp) { … }
```
The macro (shipped T.1, 2026-07-09; the attribute path is
`lumen_test::test`, not the earlier `lumen::test` — user test code depends
on `lumen-test` directly) builds the app under test from `main_app()` in
scope (`use my_app::main_app;` — the `lumen new` convention), or an explicit
`app(expr)` option. `platform(...)` marks the test `#[ignore]` (platform
runners are orchestrated externally; run with `--ignored`). Construction
path: `TestApp::with_config(app, size, scale, theme)`.

## 2. API surface

```rust
impl TestApp {
    pub fn locator(&self, selector: &str) -> Locator;       // grammar: 03 §2
    pub async fn pump_until_idle(&mut self);                 // settle layout/effects/animations
    pub fn clock(&mut self) -> &mut TestClock;               // virtual time: advance(ms), pause animations
    pub async fn screenshot(&mut self) -> RgbaImage;
    pub async fn expect_screenshot(&mut self, name: &str);   // golden compare, §4
    pub fn tree(&self) -> SemanticsDoc;                      // typed view of 03 §1
    pub async fn run_command(&mut self, name: &str, args: Value) -> Value;
}

impl Locator {
    // actions (auto-waiting, §3)
    pub async fn click(&self); pub async fn dblclick(&self); pub async fn right_click(&self);
    pub async fn fill(&self, text: &str);                    // clear + type via IME path
    pub async fn type_text(&self, text: &str);               // append keystrokes
    pub async fn press(&self, chord: &str);                  // "Ctrl+Enter"
    pub async fn hover(&self); pub async fn focus(&self);
    pub async fn scroll_into_view(&self);
    pub async fn drag_to(&self, target: &Locator);
    pub async fn set_value(&self, v: impl Into<Value>);      // sliders, selects
    // queries
    pub async fn text(&self) -> String; pub async fn value(&self) -> Value;
    pub async fn bounds(&self) -> Rect; pub async fn count(&self) -> usize;
    pub async fn style(&self, prop: &str) -> StyleValue;     // canonical form, 04 §7
    pub fn nth(&self, i: usize) -> Locator; pub fn first(&self) -> Locator;
}

// assertions (ALL auto-retrying on the virtual clock until pass or timeout — T.2)
expect(loc).to_exist().await;            expect(loc).to_be_visible().await;
expect(loc).to_have_text("…").await;     expect(loc).to_contain_text("…").await;
expect(loc).to_have_value(v).await;      expect(loc).to_be_disabled().await;
expect(loc).to_be_focused().await;       expect(loc).to_have_state("checked").await;
expect(loc).to_have_count(n).await;      expect(loc).to_have_style("background", "#1a73e8ff").await;
expect(loc).to_have_bounds_within(rect, tol).await;
```

## 3. Auto-wait semantics (shared verbatim by agent actions, 03 §3)
Before acting, poll every 10 ms (virtual-clock aware) until ALL hold, or fail `Timeout` at 5 s (configurable per test/call):
1. selector resolves to exactly one node (0 → keep waiting; >1 → fail `Ambiguous` immediately with candidates),
2. node VISIBLE, not `disabled`, opacity > 0, on-screen after auto `scroll_into_view`,
3. no pending layout, no running enter-animation on the node, event queue drained,
4. node's bounds stable across two consecutive frames.
Failures return structured errors with the closest-match suggestions and a tree snapshot attached to the trace.

## 4. Golden screenshots
- Stored at `tests/golden/<renderer>/<test_name>[.<tag>].png` (`renderer` = `cpu` or `gpu-<platform>`).
- CPU comparisons are **exact** (bit-identical; the CPU renderer is deterministic by contract 02 §7). GPU comparisons use perceptual diff: per-pixel ΔE in Oklab ≤ 2.0 and ≤ 0.1% of pixels differing; thresholds overridable per assertion.
- On mismatch: write `<name>.actual.png` and `<name>.diff.png` (differing pixels red over a dimmed base — T.3) next to the golden and fail with their paths. `LUMEN_UPDATE_GOLDENS=1 cargo test` re-records; CI never sets it.
- Perceptual compares use `TestApp::expect_screenshot_within(name, tol)` with `lumen_render::diff::Tolerance` (`PARITY`: ΔE 0.04 / 0.5% budget; `AA`: ΔE 0.04 / 4% seam budget) — the same implementation the R0 GPU-parity harness uses (T.3).
- Determinism requirements for tests: virtual clock auto-pauses animations at `pump_until_idle` unless the test advances time explicitly; system fonts are never used — the test harness bundles Noto Sans/Noto Sans CJK/Noto Color Emoji and forces them.

## 5. Traces
Every test writes `target/lumen-traces/<test>.trace.jsonl` (one JSON event per line: input events, signal writes, rebuild scopes, layout passes, frames with damage rects, tree snapshots at each action, assertion results). On failure, the last screenshot + tree snapshot are embedded. The trace format is `lumen-trace/1`, documented in `lumen-test/trace.md`; `session.exportTest` (03 §3) and the M4 inspector consume the same format.

## 6. Runners
- `cargo test` → headless CPU.
- `lumen test --platform gpu` → headed/offscreen wgpu on the host.
- `lumen test --platform android|ios_sim` (M3) → builds, installs to emulator/simulator, proxies the same TestApp API over the dev socket. Test code is identical across platforms.
