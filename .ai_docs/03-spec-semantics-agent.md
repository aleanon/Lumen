# 03 — Semantics, Selectors & Agent Protocol (normative)

The semantic tree is the single source of truth for accessibility, test locators, and agent observation. This document defines its JSON schema, the selector language over it, the `lumen-agent` JSON-RPC protocol, and the dev-server wire protocol.

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

Rules:
- Pure layout nodes (Row/Column/Padding/etc.) with no semantic contribution are **elided**: their children splice into the parent's children. `ui.getTree {"raw": true}` returns the unelided tree.
- Roles (closed set, extend via decision log): `window, button, checkbox, radio, switch, slider, text_input, text, image, link, list, list_item, table, row, cell, column_header, tab_list, tab, tab_panel, menu, menu_item, dialog, alert, tooltip, progress, group, scroll_area, tree, tree_item, combo_box, generic`.
- Every focusable leaf must have a non-empty `label` or `value`; otherwise diagnostic `W0301`.
- `bounds` here ≡ SoA `bounds` ≡ `ui.getLayout` — one source of truth (02 §5).
- AccessKit mapping (M4): each role/state maps onto AccessKit equivalents; the table lives in `lumen-core/a11y-map.md` and is part of that task's DoD.

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

Resolution semantics shared by tests and agent: a selector resolves to all matches; actions require exactly one match **after auto-wait** (see 05 §3) and return error `AmbiguousSelector` (with the match list) or `NotFound` (with nearest-miss suggestions) otherwise.

## 3. `lumen-agent` protocol

Transport: JSON-RPC 2.0 over WebSocket, default `ws://127.0.0.1:9230/agent`. The same methods are exposed as MCP tools (names below = MCP tool names with `.` → `_`). Multiple clients allowed; actions serialized in arrival order. `--token` option requires `Authorization: Bearer` — default on for non-loopback binds.

### Observation
| Method | Params | Result |
|---|---|---|
| `ui.getTree` | `{ raw?: bool, selector?: string }` | semantics doc (subtree if selector) |
| `ui.screenshot` | `{ selector?: string, format?: "png", annotate?: bool, max_width?: int }` | `{ image_base64, width, height, annotations?: [{node, id, bounds}] }` |
| `ui.getLayout` | `{ selector }` | `{ bounds, content_bounds, padding, border, margin }` |
| `ui.getStyles` | `{ selector, properties?: [string] }` | computed style map (04 §7 value serialization) |
| `app.logs` | `{ since?: seq, level?: "debug"\|"info"\|"warn"\|"error" }` | `{ entries: [{seq, ts, level, target, message}] }` |
| `app.diagnostics` | `{}` | `{ diagnostics: [Diagnostic] }` (02 §9) |
| `app.perf` | `{}` | `{ frame_ms_p50, frame_ms_p95, frames_rendered, dropped, node_count, mem_bytes }` |
| `state.get` | `{ key?: string }` | state store snapshot (whole or one key) as JSON |

`annotate: true` overlays each interactive node's short numeric tag on the image and returns the tag→node table — for vision-model grounding.

### Action (same synthesized-input path as lumen-test; everything here is reproducible as a test)
| Method | Params |
|---|---|
| `input.click` | `{ selector?, pos?: {x,y}, button?: "left"\|"right"\|"middle", count?: int }` |
| `input.type` | `{ selector, text, clear?: bool }` (focuses first; goes through IME path) |
| `input.key` | `{ keys: string }` chord syntax: `"Ctrl+Shift+P"`, `"Tab"`, `"Enter"` |
| `input.scroll` | `{ selector?, dx?, dy?, to?: "top"\|"bottom"\|{x,y} }` |
| `input.drag` | `{ from: selector\|pos, to: selector\|pos, steps?: int }` |
| `input.gesture` | `{ kind: "tap"\|"long_press"\|"pan"\|"pinch", ... }` (full params land in M3) |
| `app.setValue` | `{ selector, value }` (uses the node's `set_value` action) |
| `app.command` | `{ name, args }` — invokes commands the app registered via `cx.register_command` |

Action results: `{ ok: true, node: "node-42" }` or structured error `{ code: "NotFound"\|"Ambiguous"\|"NotActionable"\|"Timeout", detail, candidates? }`. Actions auto-wait per 05 §3 (default 5 s, override per call with `timeout_ms`).

### Sessions & events
| Method | Purpose |
|---|---|
| `events.subscribe` | `{ kinds: ["tree","input","reload","log","diagnostic"] }` → server notifications `event.*` with monotonic `seq` |
| `session.start` / `session.stop` | begin/end recording of all actions + observations |
| `session.exportTest` | `{ name }` → Rust source for a `lumen-test` test reproducing the recorded actions with auto-generated assertions (final tree snapshot + screenshots at marked points) |
| `reload.apply` | dev builds only: `{ tier: 1\|2\|3 }` force a reload; result mirrors the reload event |

Reload event payload: `{ tier, status: "ok"|"error", duration_ms, components_swapped: [string], state_preserved: bool, diagnostics: [Diagnostic] }`.

## 4. Dev-server wire protocol (CLI ⇄ app)

Dev builds embed a client that connects out to `lumen dev`'s server (`LUMEN_DEV_ADDR` env; adb reverse-forwarded on Android). Messages: length-prefixed JSON frames.

Server→app: `style_update {path, bytes}`, `shader_update {path, bytes}`, `asset_update {path, bytes}`, `dylib_update {target_triple, version, bytes}` (tier 2), `restart_request` (tier 3), `ping`.
App→server: `hello {pid, platform, lumen_version, abi_hash}`, `reload_result {…}` (same payload as above), `log {…}`, `diagnostic {…}`, `state_snapshot {bytes}` (tier-3 handoff), `pong`.
`abi_hash` is a hash of the compiler version + core crate fingerprints; the server downgrades tier 2 → 3 automatically when hashes mismatch.

The agent WebSocket (§3) is served by the **CLI dev server** and proxied over this socket, so agents talk to one stable endpoint regardless of where the app runs (desktop, emulator, simulator).
