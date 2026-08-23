# Lumen diagnostic registry

Every node-anchored diagnostic must carry its node. Use
`Diagnostic::with_target(handle, id)` — `handle` (path-derived, always
available) is what makes two findings of the same code distinguishable, and
`node` (the author's `#id`) rides along when the author named the element.
`with_handle` alone is correct where there is no author id by definition, as in
`W0301`. A finding about *several* nodes at once (`W0001`) legitimately anchors
to neither.

Diagnostic codes are **stable API** (ADR-019): agents pattern-match on them.
Never reuse or renumber a code. `E####` = error, `W####` = warning. Each code
has exactly one `pub const` in `lumen_core::codes`.

When a new code is needed, append a row here and add the matching const in the
same commit. Codes are assigned by `02-spec-core.md §9` and `04-spec-lss-styling.md §9`.

| Code  | Severity | Meaning                                              | Source spec |
|-------|----------|------------------------------------------------------|-------------|
| W0001 | warning  | Duplicate `StableId` in a window (first match wins)  | 02 §2, §9   |
| W0002 | warning  | Dropped unknown state field on snapshot restore      | 02 §4, §9   |
| E0101 | error    | `.lss` parse error                                   | 02 §9, 04 §9|
| E0102 | error    | Unknown style property (with did-you-mean)           | 02 §9, 04 §9|
| E0103 | error    | Style value type mismatch (expected type)            | 04 §9       |
| E0104 | error    | Unknown `$token` reference                            | 04 §9       |
| W0103 | warning  | Layout overflow                                       | 02 §9       |
| W0104 | warning  | Rendered ink clipped by its own box (e.g. a too-small line-height cutting descenders) | 02 §9 |
| W0105 | warning  | Interactive node laid out with zero area — clickable but invisible/unhittable | 02 §9 |
| W0106 | warning  | Node declares a semantic `Action` it does not implement (W2) | 02 §9, 03 §1|
| W0107 | warning  | `.lss` property parses but is not applied — the declaration has no effect (SD5.2) | 04 §9 |
| W0108 | warning  | Scroll container laying out many children directly; consider `VirtualList` (VL1) | 02 §9, 02 §10 |
| W0109 | warning  | `.lss` property is implemented but does not accept this **value** — the declaration has no effect (SD5.x; the other half of W0107) | 04 §9 |
| W0110 | warning  | Element needs a sprite past the portable texture limit; the renderer downscales or clamps it (oversize shadow/asset/frame) | 02 §9 |
| W0111 | warning  | Node has real area but is effectively transparent (own × inherited opacity ≈ 0) — occupies space and answers the tree, but nothing is on screen | 02 §9 |
| W0112 | warning  | Node is laid out entirely outside the window viewport (parent-relative overflow is W0103) | 02 §9 |
| W0115 | warning  | Active renderer backend has a known rendering defect (GL: gradients render as nothing, silently) | 02 §9 |
| E0201 | error    | Shader compile error                                  | 02 §9       |
| W0301 | warning  | Missing semantics on a focusable leaf (no label/value)| 02 §9, 03 §1|
| W0303 | warning  | Text contrast below the legibility floor, measured with APCA against the composited backdrop (WCAG 1.4.3) | 02 §9, 03 §1|
| W0302 | warning  | Deprecated `node-<index>` agent handle accepted; use `nx-<hex>` (ID2 alias window) | 03 §1, §2 |
| W0401 | warning  | Missing translation for a message key in the active locale (T5.3) | 02 §9 |
| W0402 | warning  | Tofu — shaped text contains `.notdef` glyphs no registered font covers (T.4) | 02 §9 |
| E0701 | error    | A build/layout/paint panic was contained; previous frame kept, app alive (T7.3) | 02 §9 |
| E0702 | error    | An **un**contained panic crossed the crash-report hook (E.3); process is going down | 02 §9 |

### Next free codes

Append here when allocating, so two workstreams don't collide (this registry
drifted to 9 documented rows against 16 defined consts once already, and a
proposed `W0105` for parse-only `.lss` properties collided with the live
zero-area-node code):

- `W01xx` layout/render: next free is **W0113** (W0114/W0115 allocated to the O phase)
- `W03xx` semantics: next free is **W0304**
- `W04xx` i18n/text: next free is **W0403**
- `E01xx` styling: next free is **E0105**
- `E07xx` panics: next free is **E0703**
