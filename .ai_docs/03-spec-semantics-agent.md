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
- AccessKit mapping: the role/state map + `TreeUpdate` builder exist (`lumen-widgets/src/a11y.rs`, table in `lumen-core/a11y-map.md`); the OS platform adapter is planned (plan P.4).

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

Resolution semantics shared by tests and agent: a selector resolves to all matches; actions require exactly one match and return `Ambiguous` (with the match list) or `NotFound` (with nearest-miss candidates) otherwise. **Gotchas:** the runtime ids `ui.getTree` returns (`node-42`) are *not* valid selectors yet (planned, C.3) — re-derive a `#id`/role/text selector; `ident` treats `.` as a class delimiter, so ids must be `[a-z0-9-]` (a dotted id `#faq.returns` parses as id `faq` + class `returns`). Headless `lumen-test` actions auto-wait per 05 §3; **live agent actions do not auto-wait yet** (planned, C.1) — poll `ui.getTree` after acting.

## 3. `lumen-agent` protocol — as implemented

**Transport.** Newline-delimited JSON-RPC 2.0 over **plain TCP**, served by
`lumen-shell` when built with the (default-off) `agent` feature and
`LUMEN_AGENT_ADDR` is set (default `127.0.0.1:9230`; `just run-agent <name>`).
One JSON object per line, one reply line per request; requests are bridged
onto the UI thread and dispatched against the live runtime, and the window
redraws after each action. A WebSocket transport (`serve_one`/
`serve_one_session`, tungstenite) exists and is used by the conformance
tests; nothing serves it in the live shell. `mcp_manifest()` exports a static
MCP tool list; there is **no MCP server yet** (planned, C.5). Loopback only;
no auth (bearer tokens planned with non-loopback binds, C.5).

### 3.1 Observation

| Method | Params | Result |
|---|---|---|
| `ui.getTree` | `{ raw?: bool }` | semantics doc per §1 (elided unless `raw`) |
| `ui.getLayout` | `{ selector }` | `{ bounds, ink?, clipped?, text_metrics?, deps? }` |
| `ui.getStyles` | `{ selector }` | computed style map (04 §7 serialization) |
| `ui.screenshot` | `{ annotate?: bool }` | `{ image_base64, width, height, annotations?: [{node, id, bounds}] }` — full frame |
| `ui.screenshot` | `{ selector, scale?: f64 = 4, overlay?: bool = true }` | zoomed crop of one element; overlay draws the box (blue) and ink (red) outlines — a defect magnifier |
| `ui.lint` | `{}` | `{ findings: [{code, message}] }` (layout/contrast audits: W0103/W0104/W0105/W0301, WCAG) |
| `app.diagnostics` | `{}` | `{ diagnostics: [Diagnostic] }` (02 §9) |
| `app.perf` | `{}` | **stub** — hardcoded zeros + `node_count`; real `FrameStats` values planned (C.2). Wall-clock around actions instead |
| `ui.probe` | `{ x, y }` | `{ color: [r,g,b,a] }` at physical px |
| `ui.probeRegion` | `{ x, y, w, h }` | `{ uniform: [r,g,b,a] \| null }` |
| `ui.getDeps` | `{ selector }` | signals the node depends on, per-prop |
| `ui.whatDependsOn` | `{ signal }` | nodes that would patch/rebuild if the signal changed (no write) |
| `ui.lastChange` | `{}` | what the last pump did: `idle` / `patch` / `rebuild` + patched nodes |
| `ui.getMenu` | `{}` | the app's `MenuModel` |
| `app.systemRequests` | `{}` | queued portable `SystemRequest`s |
| `ui.getWindows` | `{}` | app-declared `WindowDesc` list (shell is single-window) |
| `clipboard.read` | `{}` | `{ text }` (runtime clipboard — in-memory model) |

The reactive-introspection trio (`getDeps` / `whatDependsOn` / `lastChange`)
is the agent-facing projection of the dependency graph: predict which nodes a
signal write touches, then confirm what the pump actually did.

### 3.2 Actions

Synthesized into the same input queue as OS input — every action is
reproducible as a test. Results: `{ ok: true, node: "node-42" }` or a
JSON-RPC error (`-32601` unknown method; `-32000` with the resolver's
`NotFound { nearest }` / `Ambiguous { candidates }`; structured error codes
planned, C.4).

| Method | Params | Notes |
|---|---|---|
| `input.click` | `{ selector }` | pointer down+up at the node's center |
| `input.invokeAction` | `{ selector, action?: string = "click" }` | geometry-free: runs the node's retained handler (`click`/`focus`/`dismiss`/…) — robust under overlap/transforms |
| `input.type` | `{ selector, text }` | click-to-focus, then committed `TextInput`; **appends** (`clear` planned, C.4) |
| `input.key` | `{ keys }` | chord syntax `"Ctrl+Shift+P"`; named keys: Tab Enter Space Escape Backspace Delete Arrow* Home End PageUp PageDown, plus single characters |
| `input.scroll` | `{ selector?, dy }` | vertical only (`dx`/`to` planned, C.4) |
| `input.drop` | `{ selector, … }` | external file/text drop onto a node |
| `input.setLocale` | `{ locale }` | switches locale incl. RTL mirroring |
| `menu.invoke` | `{ id }` | invokes an enabled menu item |
| `clipboard.write` | `{ text }` | runtime clipboard |

### 3.3 Sessions (record → export)

`Session::dispatch` wraps the method set, records `input.*` steps, and adds:

| Method | Purpose |
|---|---|
| `session.assertText` | assert a node's text (recorded as an assertion) |
| `session.assertState` | assert a node's semantic state |
| `session.exportTest` | `{ name }` → standalone `lumen-test` Rust source reproducing the recorded steps + assertions (compiles under `cargo test`) |

**Availability caveat:** the live shell currently routes plain `dispatch`,
so `session.*` works only on the WebSocket test path; routing the shell
through `Session` is planned (C.3).

### 3.4 The live-window loop (operational contract)

1. `just run-agent <name>` → poll the TCP port for readiness (no handshake;
   port-0 + discovery file planned, C.8).
2. Observe (`ui.getTree` / screenshots / lint) → act (`input.*`) → **re-query
   to verify** (no auto-wait yet; poll until the expected state appears).
3. Prefer structural assertions (`getTree` states, `getLayout`) over pixels;
   screenshots verify layout/appearance (note: the bundled font renders
   decorative glyphs as tofu — don't assert iconography from pixels).
4. Teardown: `pkill -f "<name>-win"` (`app.quit` planned, C.8).

### 3.5 Planned (not yet implemented — do not call)

Each item carries its remediation-plan task. `app.logs` (C.2) · `state.get`
(C.4) · `events.subscribe` + `event.*` notifications (C.4) · `input.drag`
node-to-node (C.4) · `input.hover` (C.4) · `input.gesture` (C.4) ·
`app.setValue` (C.4) · `app.command` / `cx.register_command` (C.4) ·
`session.start`/`session.stop` (C.4) · `reload.apply` (C.4) · auto-wait +
`timeout_ms` on all actions and `ui.waitFor` (C.1) · `input.click`
`{pos, button, count}` (C.4) · `input.type {clear}` (C.4) · `input.scroll`
`{dx, to}` (C.4) · `ui.getTree {selector}` subtree (C.4) · `ui.screenshot`
`{max_width}` (C.4) · runtime `node-N` ids as selectors (C.3) · real
`app.perf` from `FrameStats` (C.2) · MCP server + packaged client
`lumen agent call` (C.5) · bearer-token auth (C.5) · CLI-hosted endpoint
(`lumen agent serve`, C.8).

## 4. Dev-loop wiring (per ADR-D2)

**Implemented today (in-process, no socket):**
- **Styles (tier 1):** the shell watches the file named by `LUMEN_WATCH_LSS`
  (`notify` watcher; `just run-hot <name>`) and applies it live —
  `set_stylesheet` rejects a broken sheet atomically, keeps the old one, and
  reports diagnostics. The CLI has the same watcher for headless use
  (`watch_file`/`tier1_reload`), emitting a `ReloadResult { tier, status,
  duration_ms, diagnostics }` JSON shape.
- **Tier-2 swap mechanics:** cdylib registry + `libloading` swap with an
  `lumen_abi_hash` gate and tier-3 downgrade exist as a tested library
  (`lumen-cli/src/hotpatch.rs`, fixtures `crates/fixtures/hot_*`); there is
  no live rebuild-and-push driver yet.
- **Tier-3 state restore:** `AppSnapshot` + `run_headless_restored`
  round-trip signals/scroll/focus; no process-level restart driver yet.

**Deferred (ADR-D2, 2026-07-08):** the length-prefixed socketed dev-server
protocol (`LUMEN_DEV_ADDR`, `style_update`/`shader_update`/`asset_update`/
`dylib_update`/`restart_request`/`ping` server→app; `hello {abi_hash}`/
`reload_result`/`log`/`diagnostic`/`state_snapshot`/`pong` app→server;
automatic tier-2→3 downgrade on `abi_hash` mismatch) is **design, not
implementation**. It is built when its first consumer lands — the live
tier-2 push (plan C.7), device test proxying (plan P.1), or the web agent
bridge (plan P.2) — and this section then becomes normative again. The
message vocabulary above is preserved as that design.
