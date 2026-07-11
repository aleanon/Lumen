---
name: verifying-apps
description: Use to verify that a Lumen app, screen, or feature behaves correctly — headless tests, golden screenshots, and driving the live window over the agent endpoint. Encodes the verification ladder, the implemented agent-method set (not the spec's aspirations), the no-auto-wait poll pattern, selector traps, the tofu doctrine, and port lifecycle. Applies whenever you changed app/widget behavior and must prove it works, or are asked to test/verify/screenshot a Lumen app.
---

# Verifying a Lumen app

Lumen's core promise is that an agent can verify its own work. There are
four rungs; climb only as high as the change demands. **Every rung asserts
on state + semantics + bounds first, pixels second.**

| Rung | Command | Proves | Use when |
|---|---|---|---|
| 1. Headless smoke | `cargo run -p <name>` → writes a PNG → `Read` it | builds, lays out | any visual change (cheapest sanity) |
| 2. Headless test | `cargo test -p <crate> --lib <mod>` with `TestApp`/`Headless` | logic, state, semantics, bounds | always — this is the regression artifact |
| 3. Golden | `expect_screenshot(name)` | exact pixels (CPU renderer) | styling/paint changes |
| 4. Live window | `just run-agent <name>` + `scripts/agent_client.py` | real shell: GPU, hit-testing, focus | new interactions; anything headless can't catch |

## Rung 2 — headless tests (the regression artifact)

Two APIs, same runtime:

- **Widget-level** (in `#[cfg(test)]` next to the code): `App::new(...)`
  `.run_headless(size)` → `pump`/`inject` → assert signals +
  `node_bounds_by_id` + `assert_view_coherent()`. Full pattern in the
  `writing-widgets` skill, Step 5.
- **App-level** (integration test): `lumen_test::TestApp`:

```rust
use lumen_test::{block_on, expect, TestApp};

block_on(async {
    let mut app = TestApp::new(main_app());            // or with_options(size, theme)
    app.pump_until_idle().await;
    app.locator("#save").click().await.unwrap();
    expect(app.locator("#status")).to_have_text("Saved").await.unwrap();
});
```

**Reality checks (as of 2026-07; plan T.2 closes the rest):**
- **`#[lumen_test::test]` exists (T.1)**: `async fn t(mut app: TestApp)`
  with options `size(w, h)`, `scale(f)`, `theme(dark)`, `app(expr)`
  (default: `main_app()` in scope), `platform(name)` (⇒ `#[ignore]`).
  The manual `#[test]` + `block_on` form remains fine.
- **Every `expect` assertion auto-retries** (T.2) on the virtual clock —
  time-driven and animated conditions settle inside the assertion; no
  explicit `clock().advance` needed. `Ambiguous`/`Parse` still fail fast.
- Locator has `click/right_click/fill/type_text/press/hover/focus/
  dblclick/drag_to/set_value` + queries and `to_be_visible`. **`fill`
  replaces since C.4a** (select-all + commit — full editors; the pre-IME
  `text_field_basic` still appends); `type_text` appends. **No
  `scroll_into_view` yet** (pairs with `input.scroll {to}`, plan C.4b).
- Headless locator *actions* do auto-wait (10 ms polls, 5 s, virtual-clock
  aware) with `NotFound{nearest}`/`Ambiguous{candidates}` errors.
- Animations: never sleep — `app.clock().advance(ms)` is deterministic.

**The `--lib` trap:** `cargo test -p lumen-widgets <name>` without `--lib`
matches integration-test *files*, runs **zero** of your unit tests, and
still prints `ok`. Always `--lib <module>` for widget module tests.

**Traces:** every lumen-test run writes
`target/lumen-traces/<test>.trace.jsonl` (inputs, rebuild scopes, damage,
tree snapshots; failures embed a screenshot + tree). Read it when a failure
message isn't enough.

## Rung 3 — goldens

- CPU renderer only, **bit-exact**. Stored under `tests/golden/cpu/`.
- Record/update: `LUMEN_UPDATE_GOLDENS=1 cargo test -p <crate>`.
- On mismatch you get `<name>.actual.png` **and `<name>.diff.png`** (red =
  differing pixels over a dimmed base — T.3): `Read` golden/actual/diff to
  diagnose.
- Perceptual (ΔE Oklab) compares: `expect_screenshot_within(name,
  Tolerance::PARITY|AA)` — for GPU-derived frames or intentional AA noise;
  same artifacts on failure.
- The live GPU path intentionally diverges on AA/blended pixels
  (linear-light blending) — never compare a live screenshot against a CPU
  golden byte-wise.

## Rung 4 — the live window

```bash
just run-agent <name> &          # window + JSON-RPC on 127.0.0.1:9230
python3 scripts/agent_client.py wait-port
python3 scripts/agent_client.py tree            # what the agent can see/do
python3 scripts/agent_client.py click '#save'
python3 scripts/agent_client.py screenshot /tmp/after.png
```

Also available (C.8b): `lumen inspect` — pretty semantic tree straight
from the discovered endpoint (`lumen inspect '#save'` → styles+layout for
one node); `lumen agent serve` — launch the current crate with the agent
on an ephemeral port + discovery file; `lumen test --platform gpu` — run
the `platform(gpu)`-ignored tests with `LUMEN_RENDERER=wgpu` (name your
gpu tests with `gpu` in them: that's the cargo filter).
Also available (C.5): `lumen agent call <method> ['{json}']` (the CLI
one-shot, auto-discovers the address) and `lumen agent mcp` (an MCP stdio
server proxying to the live window — for MCP-speaking clients).

Or the library for a verify loop:

```python
import sys; sys.path.insert(0, "scripts")
from agent_client import AgentClient, wait_for_port

wait_for_port()
with AgentClient() as c:
    c.screenshot("/tmp/before.png")
    c.rpc("input.click", selector='button:text-contains("Return policy")')
    n = c.wait_until(                                   # ← the auto-wait
        lambda t: c.find(t, role="button", label_contains="return policy"),
        lambda n: "expanded" in n["states"])
    c.screenshot("/tmp/after.png")
```

Then `Read` the PNGs. **Verify the state structurally (`getTree` states /
`getLayout`), confirm the look from the pixels.**

### Implemented method cheat sheet

The normative list is `.ai_docs/03-spec-semantics-agent.md` §3 (rewritten
2026-07 to match the code — trust it; §3.5 lists what does NOT exist yet).
The ones you'll actually use:

| Verb | Notes |
|---|---|
| `ui.getTree {raw?}` | roles/labels/bounds/states/actions/ids |
| `ui.getLayout {selector}` | bounds + **ink** + `clipped` + text metrics |
| `ui.screenshot {}` / `{selector, scale}` | full frame / zoomed element crop with box+ink overlay |
| `ui.lint` / `app.diagnostics` | overflow W0103, clip W0104, zero-area W0105, contrast |
| `ui.getDeps` / `ui.whatDependsOn` / `ui.lastChange` | *why* did it update — predict, act, confirm idle/patch/rebuild |
| `input.click/hover/type/key/scroll {selector,…}` | click takes `button`/`count` (double-click); type takes `clear: true` (full editors); scroll takes `dx`+`dy` (C.4a) |
| `state.get {key?}` / `ui.getTree {selector}` / `ui.screenshot {max_width}` | store snapshot; subtree-only reply; downscaled frame for vision budgets (C.4a) |
| `input.invokeAction {selector, action}` | geometry-free — use when overlap/transform makes clicks flaky |
| `input.drag {from, to, steps?}` | node-to-node pointer drag (sliders, reorder, panes) (C.4b) |
| `input.gesture {selector, kind, …}` | tap/double_tap/long_press/pan/pinch as recognized gestures (C.4b) |
| `app.setValue {selector, value}` | semantic text replacement — text controls only; sliders via drag (C.4b) |
| `app.command {name}` | invoke a `cx.register_command` handler, no geometry (C.4b) |
| `reload.apply {source}` | live stylesheet swap, atomic accept/reject + diagnostics (C.4b) |
| `session.start` / `session.stop` | bracket what `session.exportTest` emits (C.4b) |

### Live-window traps (each one has burned an agent)

- **Auto-wait covers existence; settling is a separate call.** Since
  C.1a, selector actions wait (10 ms polls, `timeout_ms` param, default
  5 s) for the node to exist, be non-zero-sized, not disabled — including
  nodes that appear from async results. `ui.waitFor {selector, state?,
  text?}` is the explicit wait. **Animations settle via `ui.waitSettled
  {timeout_ms?}`** (C.1b): it advances the clock and returns once nothing
  is `animate()`-continuous and no future `wake_at` is pending (a bare
  `now_ms()` read doesn't count — that schedules nothing to wait for). A
  forever-spinner therefore times out, readably; verify final state via
  `ui.waitFor` after `ui.waitSettled`.
- **`node-N` ids ARE selectors since C.3** — act on exactly the node
  `ui.getTree`/`ui.waitFor` returned. They're per-rebuild runtime ids
  though: re-query after structural changes; prefer a stable `#id` in
  committed tests. Ambiguous/NotFound errors are now readable and list
  `node-N` candidates.
- **Dotted ids are unselectable.** `#faq.returns` parses as id `faq` +
  class `returns`. Ids must be `[a-z0-9-]`. If `tree` shows a dotted id,
  that's an app bug — fix the id, don't work around it.
- **`app.perf` is real since C.2**: `{frame_ms_p50, frame_ms_p95,
  frames_rendered, node_count}` over the last ≤120 painted frames.
  `app.logs {since?}` returns the diagnostic ring (handler
  `rt.log(level, msg)` entries, E0701 panics, stylesheet rejections) —
  page with `since` = last seq + 1.
- **Tofu doctrine.** The bundled font lacks decorative glyphs (▼▶ arrows,
  most symbols) — they render as boxes while semantics report the intended
  character, and `ui.lint` does **not** flag it. So: verify *behaviour*
  from the tree, verify *layout* from pixels, and never assert iconography
  from either without checking the other. If the UI needs an icon, draw it
  as a shape (`widgets::canvas`).
- **Record→export works live since C.3**: the shell routes through a
  recording `Session` — after exploring, `session.assertText`/
  `assertState` then `session.exportTest {fnName, appExpr}` returns a
  compilable `lumen-test` source reproducing the run. Commit it as the
  regression test.

### Port lifecycle (C.8a)

- **Parallel-safe launch:** `just run-agent <name> 127.0.0.1:0` binds an
  ephemeral port; the bound address lands in `target/lumen-agent.addr`
  (override with `LUMEN_AGENT_ADDR_FILE`) and `agent_client.py` picks it
  up automatically — no port bookkeeping.
- Readiness = the port accepting connections (`wait-port`); the shell also
  prints a `{"lumen_agent_ready":true, "addr":…}` line. First launch
  compiles in release — allow ~2 min cold.
- **Teardown:** `just stop-agent <name>` — sends `app.quit` (clean event-
  loop exit) and clears the discovery file; falls back to
  `pkill -x "<name>-win"`. If you pkill manually, never `pkill -f` from a
  script whose command line contains the pattern (it kills your own
  shell) — bracket it: `pkill -f "[a]ccordion-win"`.

## Verifying *why* (the reactive layer)

When the question is "did the right thing update":

1. `ui.getDeps {selector}` — which signals this node depends on.
2. Predict: `ui.whatDependsOn {signal}` — what a write *would* touch.
3. Act, then `ui.lastChange` — was it `idle`, a paint-only `patch`, or a
   `rebuild`? A `rebuild` where you expected a `patch` is a perf bug;
   `idle` where you expected anything is a missing dependency.

In headless tests the analogue is `assert_view_coherent()` — incremental
must equal rebuild-from-scratch; run it after any interaction sequence.

## Before you call it verified

- [ ] Headless test asserting state **and** semantics/bounds (not just a
      signal) — committed, green with `--lib` where applicable.
- [ ] `assert_view_coherent` after the interaction sequence.
- [ ] Golden updated/added if paint changed; you looked at the `.actual`.
- [ ] For new interactions: one live-window loop (screenshot → act →
      `wait_until` state → screenshot), PNGs eyeballed, window killed,
      port confirmed closed.
- [ ] `ui.lint` + `app.diagnostics` clean (or findings explained).
- [ ] `cargo fmt --all && cargo clippy --workspace --all-targets` clean;
      commit per AGENT.md (incl. the doc-currency rule if behavior moved).

## References

- `.ai_docs/03-spec-semantics-agent.md` — protocol as implemented; §3.5 = not yet.
- `.ai_docs/05-spec-testing.md` — harness spec (aspirational parts marked).
- `scripts/agent_client.py` — the client all snippets here use.
- `writing-widgets` skill — widget-level test pattern + example-crate recipe.
- `debugging-lumen` skill — when verification *fails* and you need to know why.
