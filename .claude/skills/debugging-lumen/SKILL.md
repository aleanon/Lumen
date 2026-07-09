---
name: debugging-lumen
description: Use when a Lumen app misbehaves — clicks do nothing, UI stays stale after a state change, layout is wrong, an element is invisible to the agent/tests, keyboard input goes nowhere, a .lss rule seems ignored, a panic, or a perf regression. Maps each symptom to its cause and the introspection tool that proves it, in the order that converges fastest.
---

# Debugging a Lumen app

Work symptom-first (§1). If the symptom isn't listed, walk the
introspection ladder (§2). Every tool here is headless-or-live; nothing
requires a debugger.

## 1. Symptom → cause → proof

| Symptom | Most likely cause | Prove it |
|---|---|---|
| **Click does nothing** | Handler captured an owned snapshot that went stale (ADR-013) — a cloned `String`/`Vec`/value instead of the signal handle | Wrap the closure in `stable_handler!` → compile error = confirmed. Fix: capture only `Copy` handles, mutate via `sig.update` |
| Click does nothing (handler is fine) | Another node wins hit-testing — later siblings sit on top (document order) | `ui.getTree`: is the target covered? `input.invokeAction {selector}` works while `input.click` doesn't ⇒ geometry problem |
| **UI stale after a signal write** | The dependency wasn't read inside the `cx.scope` that renders it — the memo cache doesn't know | `ui.getDeps {selector}`: is the signal listed? No ⇒ move the read inside the scope. Headless: `assert_view_coherent()` fails |
| UI stale (deps look right) | Write happened outside a handler (e.g. from a worker thread) and never entered the runtime | Results must re-enter via `resource`/`Sink`; never touch `Runtime` off-thread |
| Async value never arrives | `Runtime::resource(name, future)` — the future-taking form polls once with a noop waker and never completes | Use `cx.resource(name, deps, fetch)` / `resource_blocking` (thread pool) instead. (Fixed by plan M.5) |
| **Layout wrong, state right** | Text node given explicit `height` (ignored — text sizes to glyphs), or `.lss` layout property (parse-only no-op) | `ui.getLayout {selector}` bounds vs expectation; check the property against 04 §10 / the `styling-lss` skill |
| Content visibly cut off | Real clipping | `ui.getLayout`: `ink` bigger than `bounds` + `"clipped": true`; `ui.lint` → W0104 |
| Overlapping/zero-size controls | — | `ui.lint` → W0103 overflow, W0105 zero-area interactive |
| **`.lss` rule ignored** | Property outside the applied subset, `:hover` instead of `:hovered`, nested `&` rule (dropped), or a value type mismatch (E0103 never fires — silent) | `ui.getStyles {selector}` — is the computed value there? Then the `styling-lss` skill's no-op list |
| **Element invisible to tests/agent** | No `role`/`label` (semantically invisible), or elided as a pure-layout node | `ui.getTree {raw: true}` — present in raw but not elided ⇒ add role/label |
| Selector can't find an obvious node | Dotted id (`#a.b` parses as id+class), or `node-N` used as a selector | `agent_client.py tree` shows the real ids; use `#dash-cased-id` |
| **Keyboard goes nowhere** | Focus sits on a different node than the key handler — keystrokes route to the *focused* id | `ui.getTree`: which node has `focused`? Give the click target and the editor the same stable id |
| Space/Enter doesn't activate | Node lacks `focusable: true` (focus+`on_click` gives key activation for free) | check `actions`/`focusable` in the tree |
| **Panic** | Build/layout/paint panic is contained: window keeps last frame, subtree `error_boundary` catches locally | `app.diagnostics` → `E0701`; headless: the test fails with the panic message. Clears on next clean build |
| **Perf regression** | Rebuild where a patch was expected; text churn (7 ms full CPU raster); non-virtualized list; hover storm (pointer motion currently wipes scope memos — known, plan A.1) | `ui.lastChange` after the interaction: `rebuild` vs `patch` vs `idle`; `cargo bench -p lumen-benches` + `scripts/perf_gate.sh` |
| Idle CPU not ~0 | A `cx.animate()` that never settles (spring re-targeted every frame), or a timer loop | idle for 10 s, then `ui.lastChange` — anything but `idle` means something's ticking |
| Golden mismatch | Intentional paint change vs regression | `Read` the golden and the `.actual.png` side by side (no `.diff.png` yet); re-record only when intended: `LUMEN_UPDATE_GOLDENS=1` |
| Test passes but shouldn't / runs 0 tests | Bare `cargo test -p <crate> <name>` matched integration files, not your `--lib` unit tests | rerun with `--lib` — "ok, 0 passed" is the tell |
| Icon/arrow renders as a box | Tofu: bundled font lacks decorative glyphs; semantics still report the character; lint is silent | Draw shapes via `widgets::canvas` instead (writing-widgets gotchas) |

## 2. The introspection ladder (unknown misbehavior)

Run against the live window (`just run-agent <name>` +
`scripts/agent_client.py`) or headless equivalents:

1. **`app.diagnostics`** — structured errors first (E0101 parse, E0701
   panic, W0401 i18n…).
2. **`ui.lint`** — layout/contrast audits (W0103/W0104/W0105 + WCAG).
3. **`agent_client.py tree`** — one line per node: role, id, label,
   states, actions, bounds. Most "invisible/unclickable/wrong place" bugs
   are obvious here.
4. **`ui.getLayout {selector}`** — box vs ink vs clipped; text metrics.
5. **Reactive triple** — `ui.getDeps` (what it reads),
   `ui.whatDependsOn {signal}` (what a write should touch), act, then
   `ui.lastChange` (what actually happened: idle/patch/rebuild + nodes).
   Mismatch between prediction and result localizes the bug to either the
   dependency graph or the write site.
6. **Element zoom** — `agent_client.py screenshot /tmp/z.png --selector
   '#thing' --scale 4` draws box (blue) + ink (red) outlines around the
   suspect at 4×.
7. **Traces** — headless runs write
   `target/lumen-traces/<test>.trace.jsonl`: inputs, rebuild scopes,
   damage rects, tree snapshots; failures embed screenshot+tree.
8. **Coherence oracle** — `h.assert_view_coherent()` in a headless repro:
   if incremental ≠ rebuild-fresh, it's a memoization/invalidation bug in
   the framework path, not your app.

## 3. Diagnostics that fire vs. don't (don't wait for dead codes)

- **Fire:** E0101, E0102 (did-you-mean), E0104, W0002 (state evolution),
  W0103/W0104/W0105 (via lint), E0201 (shader), W0401 (i18n), E0701
  (contained panic).
- **Never fire (defined but dead — plan W.4/B.7):** W0001 duplicate id,
  W0301 unnamed focusable *as a diagnostic* (the lint checks it), E0103
  style type mismatch. Absence of these codes proves nothing.

## 4. Minimal repro discipline

Shrink to a headless test before fixing: `App::new(|cx| …tiny build…)
.run_headless(size)` + the failing interaction + `assert_view_coherent`.
It becomes the regression test in the same commit as the fix (AGENT.md).

## References

- Skills: `verifying-apps` (the tools' happy-path usage), `styling-lss`
  (no-op property list), `writing-widgets` (gotchas §, handler table),
  `building-apps` (state rules).
- `.ai_docs/03-spec-semantics-agent.md` §3 — full method reference.
- `lumen-core/diagnostics.md` — the code registry.
