# Lumen diagnostic registry

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
| E0201 | error    | Shader compile error                                  | 02 §9       |
| W0301 | warning  | Missing semantics on a focusable leaf (no label/value)| 02 §9, 03 §1|
| W0401 | warning  | Missing translation for a message key in the active locale (T5.3) | 02 §9 |
| W0402 | warning  | Tofu — shaped text contains `.notdef` glyphs no registered font covers (T.4) | 02 §9 |
| E0701 | error    | A build/layout/paint panic was contained; previous frame kept, app alive (T7.3) | 02 §9 |
| E0702 | error    | An **un**contained panic crossed the crash-report hook (E.3); process is going down | 02 §9 |

### Next free codes

Append here when allocating, so two workstreams don't collide (this registry
drifted to 9 documented rows against 16 defined consts once already, and a
proposed `W0105` for parse-only `.lss` properties collided with the live
zero-area-node code):

- `W01xx` layout/render: next free is **W0107**
- `W03xx` semantics: next free is **W0302**
- `W04xx` i18n/text: next free is **W0403**
- `E01xx` styling: next free is **E0105**
- `E07xx` panics: next free is **E0703**
