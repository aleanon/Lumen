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
| E0201 | error    | Shader compile error                                  | 02 §9       |
| W0301 | warning  | Missing semantics on a focusable leaf (no label/value)| 02 §9, 03 §1|
