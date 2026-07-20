# 03 — Semantics, Selectors & Agent Protocol (normative)

The semantic tree is the single source of truth for accessibility, test locators, and agent observation. This document defines its JSON schema, the selector language over it, the `lumen-agent` JSON-RPC protocol, and the dev-loop wiring.

> **Re-grounded 2026-07-09** against the implementation (per the docs↔code
> audit, `docs/review-docs-vs-code-2026-07.md`). §3 now documents the
> protocol **as implemented**, including methods the previous revision
> omitted; everything not yet built is in §3.5 **Planned**, tagged with its
> remediation-plan task (`docs/plan-remediation-2026-07.md`). §4 reflects
> ADR-D2: the socketed dev-server protocol is deferred until it has a
> consumer; the current dev loop is in-process.

## 1. Semantic tree JSON schema

`Headless::semantics_json()` and `ui.getTree` return:

```json
{
  "schema": "lumen-semantics/1",
  "window": { "width": 800, "height": 600, "scale": 2.0, "focused": "node-42" },
  "root": { "$ref": "SemNode" }
}
```

`SemNode`:
```json
{
  "node": "node-42",                  // runtime id: "node-" + NodeIndex.index
  "id": "save-button",                // StableId if set, else absent
  "role": "button",
  "label": "Save",                    // accessible name (explicit or derived from text)
  "value": "42" ,                     // current value for inputs/sliders; absent otherwise
  "classes": ["primary"],
  "states": ["focused", "hovered"],   // subset of: focused hovered pressed disabled checked
                                      // unchecked mixed selected expanded collapsed readonly required invalid busy
  "bounds": { "x": 10.0, "y": 20.0, "w": 120.0, "h": 32.0 },   // window coords, post-layout
  "actions": ["click", "focus"],      // subset of: click focus blur set_value increment decrement
                                      // scroll_into_view expand collapse dismiss
  "scroll": { "x": 0, "y": 120, "max_x": 0, "max_y": 980 },    // scroll containers only
  "text_selection": { "start": 3, "end": 7 },                   // text inputs only
  "type": "Button",                   // Rust widget type name (debug aid; not for selectors in tests)
  "children": [ ... ]
}
```

Implementation note — the runtime emits additional diagnostic fields beyond
this core schema (additive, optional): `ink` (rendered ink bounds) and
`clipped` on `ui.getLayout`, `text_metrics` (line_count/box_height/ascent/
descent/line_height/content_height) for text nodes, and `deps` (the reactive
signal keys whose change re-runs the node's scope). Consumers must tolerate
unknown fields.

Rules:
- Pure layout nodes (Row/Column/Padding/etc.) with no semantic contribution are **elided**: their children splice into the parent's children. `ui.getTree {"raw": true}` returns the unelided tree.
- Roles (closed set, extend via decision log): `window, button, checkbox, radio, switch, slider, text_input, text, image, link, list, list_item, table, row, cell, column_header, tab_list, tab, tab_panel, menu, menu_item, dialog, alert, tooltip, progress, group, scroll_area, tree, tree_item, combo_box, generic`.
- Every focusable leaf must have a non-empty `label` or `value`; otherwise diagnostic `W0301`.
- `bounds` here ≡ SoA `bounds` ≡ `ui.getLayout` — one source of truth (02 §5).
- AccessKit mapping (P.4 ✅): the role/state map + `TreeUpdate` builder (`lumen-widgets/src/a11y.rs`; nodes carry bounds, declared actions, and value-for-static-text) publish to the OS through an `accesskit_winit` adapter in the shell — dormant until an AT subscribes, tree pushed after every painted frame, AT `Click` actions routed into the one input queue (`route_at_action`). Live-verified over AT-SPI on this box: app identity, names, and a `doAction('click')` that drove app state.

## 2. Selector grammar

Used by `lumen-test` locators and all agent methods taking `selector`.

```
selector   := compound (combinator compound)*
combinator := WS            // descendant
            | '>'           // direct child
compound   := part+
part       := '#' ident                  // StableId equals
            | '.' ident                  // class contains
            | role                       // role equals (bare word, e.g. button)
            | ':' state                  // state present, e.g. :focused :disabled :checked
            | ':text("…")'               // label or text content equals (after trim)
            | ':text-contains("…")'      // substring, case-insensitive
            | ':has(' selector ')'       // has a matching descendant
            | ':nth(' int ')'            // 1-based among current matches, applied last
            | '*'                        // any
```
Matching runs over the **elided** semantic tree, document order. Examples: `#save-button`, `button:text("Continue")`, `dialog .footer > button:nth(2)`, `list_item:has(:text-contains("invoice"))`.

Resolution semantics shared by tests and agent: a selector resolves to all matches; actions require exactly one match and return `Ambiguous` (with the `node-N` candidate list and advice) or `NotFound` otherwise. Agent methods additionally accept the runtime ids `ui.getTree` returns (`node-42`) as direct lookups (C.3) — act on exactly the node you observed. **Gotcha:** `ident` treats `.` as a class delimiter, so ids must be `[a-z0-9-]` (a dotted id `#faq.returns` parses as id `faq` + class `returns`). Actions auto-wait (existence/actionability/async — C.1a) both headless and live; clock-driven animation settling is not waited on yet (C.1b).

## 3. `lumen-agent` protocol — as implemented

**Transport.** Newline-delimited JSON-RPC 2.0 over **plain TCP**, served by
`lumen-shell` when built with the (default-off) `agent` feature and
`LUMEN_AGENT_ADDR` is set (default `127.0.0.1:9230`; `just run-agent <name>`).
One JSON object per line, one reply line per request; requests are bridged
onto the UI thread and dispatched against the live runtime, and the window
redraws after each action. A WebSocket transport (`serve_one`/
`serve_one_session`, tungstenite) exists and is used by the conformance
tests. **Packaged clients (C.5):** `lumen agent call <method> [json]`
(one-shot; auto-discovers the address) and `lumen agent mcp` — a real MCP
stdio server whose tools come from `mcp_manifest()` (`ui_getTree` ↔
`ui.getTree`), proxying `tools/call` onto the endpoint; point any MCP
client at it while `just run-agent` runs. **Auth (C.5):** a non-loopback
bind is refused unless `LUMEN_AGENT_TOKEN` is set; every request must then
carry `"auth": "<token>"` (the packaged clients attach it from the env).

### 3.1 Observation

| Method | Params | Result |
|---|---|---|
| `ui.getTree` | `{ raw?: bool, selector?: string }` | semantics doc per §1 (elided unless `raw`); with `selector`, just that subtree (C.4a) |
| `state.get` | `{ key?: string }` | state-store snapshot as JSON — whole store, or one signal key (C.4a) |
| `ui.getLayout` | `{ selector }` | `{ bounds, ink?, clipped?, text_metrics?, deps? }` |
| `ui.getStyles` | `{ selector }` | computed style map (04 §7 serialization) |
| `ui.screenshot` | `{ annotate?: bool, max_width?: int }` | `{ image_base64, width, height, annotations?: [{node, id, bounds}] }` — full frame; `max_width` downscales (nearest, aspect-preserving — vision token budgets, C.4a) |
| `ui.screenshot` | `{ selector, scale?: f64 = 4, overlay?: bool = true }` | zoomed crop of one element; overlay draws the box (blue) and ink (red) outlines — a defect magnifier |
| `ui.lint` | `{}` | `{ findings: [{code, message}] }` (layout/contrast audits: W0103/W0104/W0105/W0301, WCAG) |
| `app.diagnostics` | `{}` | `{ diagnostics: [Diagnostic] }` (02 §9) |
| `app.perf` | `{}` | `{ frame_ms_p50, frame_ms_p95, frames_rendered, node_count }` — rolling stats over the last ≤120 painted pumps (C.2) |
| `app.logs` | `{ since?: seq }` | `{ entries: [{seq, level, message}] }` — the runtime's diagnostic ring (handler `rt.log(level, msg)`, E0701 panics, stylesheet rejections); page with `since` = last seq + 1 (C.2) |
| `ui.probe` | `{ x, y }` | `{ color: [r,g,b,a] }` at physical px |
| `ui.probeRegion` | `{ x, y, w, h }` | `{ uniform: [r,g,b,a] \| null }` |
| `ui.getDeps` | `{ selector }` | signals the node depends on, per-prop |
| `ui.whatDependsOn` | `{ signal }` | nodes that would patch/rebuild if the signal changed (no write) |
| `ui.lastChange` | `{}` | what the last pump did: `idle` / `patch` / `rebuild` + patched nodes |
| `ui.getMenu` | `{}` | the app's `MenuModel`; items carry optional `accel` chords (`"Ctrl+O"`) which the shell matches (P.3c) |
| `app.systemRequests` | `{}` | queued portable `SystemRequest`s |
| `ui.getWindows` | `{}` | app-declared `WindowDesc` list; the shell realizes each as a real OS window over the shared store (P.3d). Agent verbs address the MAIN window; per-window addressing is future work |
| `clipboard.read` | `{}` | `{ text }` (runtime clipboard — in-memory model) |

The reactive-introspection trio (`getDeps` / `whatDependsOn` / `lastChange`)
is the agent-facing projection of the dependency graph: predict which nodes a
signal write touches, then confirm what the pump actually did.

### 3.2 Actions

Synthesized into the same input queue as OS input — every action is
reproducible as a test. **Auto-wait (C.1a, 2026-07-09):** every selector
action polls at 10 ms — pumping so deferred task results apply — until the
selector resolves to exactly one *actionable* node (non-empty bounds, not
`disabled`) or `timeout_ms` elapses (param on any action; default 5000).
`Ambiguous` fails immediately with candidates. Animation settling is the
explicit `ui.waitSettled` call (C.1b ✅), not part of per-action auto-wait. Results: `{ ok: true, node: "node-42" }` or a JSON-RPC
error (`-32601` unknown method; `-32000` with `Timeout(…)` /
`NotFound { nearest }` / `Ambiguous { candidates }`; structured error codes
planned, C.4). `events.subscribe` (push notifications) remains open —
it needs a push-transport design over the request/response loop (C.7).

| Method | Params | Notes |
|---|---|---|
| `ui.waitFor` | `{ selector, state?, text?, timeout_ms? }` | blocks until the node exists (and carries `state` / label-or-value equals `text`); the explicit wait primitive |
| `ui.waitSettled` | `{ timeout_ms? }` | C.1b: advances the clock by wall time between 10 ms polls until no `animate()` (continuous) request and no future `wake_at` remain and the reactive graph is quiescent; returns `{ settled, waited_ms }`. A bare `now_ms()` read doesn't count (nothing scheduled to wait for); a forever-animation times out readably |
| `input.drag` | `{ from, to, steps? }` | C.4b: node-to-node pointer drag — down at `from`'s center, interpolated moves (default 8), up at `to`'s center |
| `input.gesture` | `{ selector, kind, dx?, dy?, scale? }` | C.4b: `tap\|double_tap\|long_press\|pan\|pinch` — injects the recognized `GestureEvent` the touch pipeline would produce |
| `app.setValue` | `{ selector, value }` | C.4b: semantic text replacement (focus → select-all → commit); sliders/steppers: use `input.drag` |
| `app.command` | `{ name }` | C.4b: invoke a `cx.register_command` handler by name — geometry-free; unknown names list what is registered |
| `reload.apply` | `{ source }` | C.4b: tier-1 stylesheet hot reload over the wire — same atomic accept/reject as the file watcher; rejection returns the diagnostics |
| `session.start` / `session.stop` | `{}` | C.4b: bracket recording for `session.exportTest` (recording is on by default; `start` clears prior steps) |

| Method | Params | Notes |
|---|---|---|
| `input.click` | `{ selector, button?: "left"\|"right"\|"middle", count?: 1–3 }` | pointer down+up at the node's center; `count: 2` = double-click (C.4a) |
| `input.hover` | `{ selector }` | pointer move over the node — tooltips, `:hovered` (C.4a) |
| `input.invokeAction` | `{ selector, action?: string = "click" }` | geometry-free: runs the node's retained handler (`click`/`focus`/`dismiss`/…) — robust under overlap/transforms |
| `input.type` | `{ selector, text, clear?: bool }` | click-to-focus, then committed `TextInput`; `clear: true` replaces via the editor's select-all (full editors; the pre-IME `text_field_basic` appends regardless) (C.4a) |
| `input.key` | `{ keys }` | chord syntax `"Ctrl+Shift+P"`; named keys: Tab Enter Space Escape Backspace Delete Arrow* Home End PageUp PageDown, plus single characters |
| `input.scroll` | `{ selector?, dx?, dy? }` | both axes (`to: top\|bottom\|{x,y}` planned, C.4b) |
| `input.drop` | `{ selector, … }` | external file/text drop onto a node |
| `input.setLocale` | `{ locale }` | switches locale incl. RTL mirroring |
| `menu.invoke` | `{ id }` | invokes an enabled menu item **and runs the command registered under the same id** (`cx.register_command`) — the same path as a native click or accelerator (P.3c) |
| `clipboard.write` | `{ text }` | runtime clipboard |

### 3.3 Sessions (record → export)

`Session::dispatch` wraps the method set, records `input.*` steps, and adds:

| Method | Purpose |
|---|---|
| `session.assertText` | assert a node's text (recorded as an assertion) |
| `session.assertState` | assert a node's semantic state |
| `session.exportTest` | `{ name }` → standalone `lumen-test` Rust source reproducing the recorded steps + assertions (compiles under `cargo test`) |

Available on **both** the live shell (routed through a recording `Session`
since C.3 — explore the window, commit the exported test) and the
WebSocket test path.

### 3.4 The live-window loop (operational contract)

1. `just run-agent <name> [addr]` — pass `127.0.0.1:0` for an ephemeral
   port (parallel sessions never collide). The **bound** address is written
   to the discovery file (`$LUMEN_AGENT_ADDR_FILE`, default
   `target/lumen-agent.addr`) and printed as a JSON ready line;
   `scripts/agent_client.py` reads the file automatically (C.8a).
2. Observe (`ui.getTree` / screenshots / lint) → act (`input.*`) → **re-query
   to verify** (no auto-wait yet; poll until the expected state appears).
3. Prefer structural assertions (`getTree` states, `getLayout`) over pixels;
   screenshots verify layout/appearance (note: the bundled font renders
   decorative glyphs as tofu — don't assert iconography from pixels).
4. Teardown: `just stop-agent [name]` — sends `app.quit` (a shell-level
   method: replies then exits the event loop, C.8a), falling back to pkill;
   clears the discovery file.

### 3.5 Planned (not yet implemented — do not call)

Each item carries its remediation-plan task. `events.subscribe` + `event.*`
notifications (C.4b) · `input.drag` node-to-node (C.4b) · `input.gesture`
(C.4b) · `app.setValue` (C.4b) · `app.command` / `cx.register_command`
(C.4b) · `session.start`/`session.stop` (C.4b) · `reload.apply` (C.4b) ·
auto-wait for clock-driven **animation settling** (C.1b;
existence/actionability/async waiting shipped in C.1a) · `input.click
{pos}` (C.4b) · `input.scroll {to}` (C.4b) · CLI-hosted endpoint (`lumen
agent serve` proxying a managed app, C.8b). *(Shipped since the re-ground:
C.1a auto-wait + `ui.waitFor`; C.2 `app.logs` + real `app.perf`; C.3 live
`session.*`, `node-N` selectors, readable resolver errors; C.4a state.get/
subtree-getTree/max_width/hover/click-opts/scroll-dx/type-clear; C.5
`lumen agent call` + MCP stdio server + bearer auth; C.8a port-0 +
discovery + `app.quit` + `just stop-agent`.)*

## 4. Dev-loop wiring (per ADR-D2)

**Implemented today (in-process, no socket):**
- **Styles (tier 1):** the shell watches the file named by `LUMEN_WATCH_LSS`
  (`notify` watcher; `just run-hot <name>`) and applies it live —
  `set_stylesheet` rejects a broken sheet atomically, keeps the old one, and
  reports diagnostics. The CLI has the same watcher for headless use
  (`watch_file`/`tier1_reload`), emitting a `ReloadResult { tier, status,
  duration_ms, diagnostics }` JSON shape.
- **Tier-2/3 live orchestration (C.7 ✅):** `lumen dev <crate> <path>` /
  `lumen_cli::dev::Tier2Driver` — watch → `cargo build -p` → swap into the
  RUNNING host app in place (`HotComponent::swap`, `lumen_abi_hash` gated;
  host state untouched by construction); an ABI mismatch downgrades to the
  tier-3 restart driver (in-process `restart_request` + `state_snapshot`
  handoff: snapshot → fresh load → `run_headless_restored`). One
  `ReloadResult`-shaped JSON line per applied build. Verified end-to-end:
  live counter state survives both tiers (tests/tier2_driver.rs).

**Deferred (ADR-D2, 2026-07-08):** the length-prefixed socketed dev-server
protocol (`LUMEN_DEV_ADDR`, `style_update`/`shader_update`/`asset_update`/
`dylib_update`/`restart_request`/`ping` server→app; `hello {abi_hash}`/
`reload_result`/`log`/`diagnostic`/`state_snapshot`/`pong` app→server;
automatic tier-2→3 downgrade on `abi_hash` mismatch) is **design, not
implementation**. C.7's live tier-2 loop landed **in-process** (per this
ADR's preference), so the socket's remaining consumers are device test
proxying (plan P.1) and the web agent bridge (plan P.2); the section
becomes normative when one of those builds it. The
message vocabulary above is preserved as that design.
