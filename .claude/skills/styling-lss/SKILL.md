---
name: styling-lss
description: Use when writing or editing .lss stylesheets, theming a Lumen app, or deciding whether a property belongs in .lss or in Rust LayoutStyle. Encodes which properties actually render today (the parser accepts far more than the runtime applies), the working tokens/themes subset, the run-hot reload workflow, and the diagnostics that do/don't fire — so styles never silently do nothing.
---

# Styling with `.lss` — the working subset

`.lss` parses essentially the whole spec (`.ai_docs/04-spec-lss-styling.md`)
but the **runtime applies a subset**. Writing a parse-only property is a
silent no-op: no error, no effect. This skill keeps you inside the subset;
the authoritative per-property table is 04 §10 (kept current by the
doc-currency rule; regenerated as remediation Phase B lands).

## The one rule

> **Core box layout works in `.lss` (since A.2); everything else layout
> stays in Rust until Phase B.**

Styles resolve *before* layout now, so these seven work from `.lss`:
`display`, `flex-direction`, `width`, `height`, `gap` (both axes),
`padding` and `margin` (whole-side). `.lss` wins over the element's
`LayoutStyle` per-property. Two caveats: **text-bearing nodes still derive
`height` from their glyphs** (the text-height rule — size a box, put the
text in a child), and a state-part layout rule (`#x:hovered { width: … }`)
relayouts through the normal rebuild path on pointer motion.

Still Rust-only (parse-only in `.lss` until Phase B): per-side
padding/margin/border, `flex-grow/shrink/basis/wrap`, `justify-*`/
`align-*`, `min/max-*`, `aspect-ratio`, `grid-*` tracks,
`Position::Absolute` + `inset`, `overflow`.

## What works in `.lss` today

| Works (renders) | Notes |
|---|---|
| `background: <color>` | solid colors only — **no gradients** (gradients exist in Rust via `Frame`/element APIs) |
| `border: 1px solid <color>` | shorthand only; per-side `border-top` etc. are no-ops |
| `border-radius: 6px` | single value only; `4px 8px …` lists rejected |
| `color: <color>` | text color |
| `backdrop-filter: blur(8px) saturate(1.2) refraction(2) specular(0.5)` | full glass stack (refraction/specular are Lumen extensions) |
| `opacity: 0.5` | subtree compositing layer (B.3a) |
| `font-size`, `font-weight` (100–900, synthesized bold), `line-height` (multiple) | reach measure **and** paint (B.4a) — text nodes only |
| widget parts: `slider .track`, `slider .thumb`, `progress .fill` | parts are classes on the internals, scoped by the ancestor chain; custom widgets expose parts with `Element::part("thumb")` (B.7) |
| relative colors `oklch(from <color or $token> L C H)` | channel keywords `l`/`c`/`h` + `calc(l + 0.06)` (`+ - *`, left-to-right, spaces around operators); alpha from the base; `$token`s resolve inside function args/shorthand lists (B.7) |
| `@tokens { … }` / `@theme light|dark { … }` / `$token` | full token resolution, theme-scoped first |
| **Nested rules** `&:hovered { … }` / `&.class { … }` / `& > .part { … }` | flattened at parse into real rules with correct specificity (B.1) |
| Descendant/child combinators (`.card .btn`, `#x > .y`) | match the **real ancestor chain** since B.1 — before that only the rightmost compound was checked, so `dialog button` hit every button |
| `@media (width < 500) { … }` etc. | gates on the **live window** since B.2 (width/height/scale/platform/pointer); resizing re-resolves. Confirm with `ui.getStyles` after a resize |
| Specificity, `!important`, selector grammar (`#id .class role :state`, `:text(…)`, `:has(…)`, `:nth(n)`) | cascade order per 04 §2 |

Colors: `#rgb/#rrggbb/#rrggbbaa`, `rgb(r g b)` (no alpha arg), `oklch(L C H)`
numeric only.

## Silent no-ops — do NOT use (until the noted plan task lands)

- **All layout properties** (A.2) — see the one rule.
- Background **gradients**, `shadow`, `blend-mode`, `filter`, `clip`,
  `transform`, `z-index`, `visibility`, `cursor` (B.3).
- Remaining typography: `font-family/style/features/variation`,
  `letter-spacing`, `text-align/overflow/wrap/decoration`,
  `selection-color` (B.4). Use Rust `TextStyle`/`Label` setters instead
  (`font-size`/`font-weight`/`line-height` work — see the works table).
- `transition:` / `animation:` / `@keyframes` — parsed, never played (B.5).
  Motion comes from Rust: `motion::spring` + `cx.animate()`.
- `@media container(...)` — parse error until B.2b (window-level `@media`
  works — see the works table).

## State selectors (full vocabulary since B.6a)

Interaction states match under both spellings — `:hovered`/`:hover`,
`:focused`/`:focus`, `:pressed`/`:active` — and **every semantic widget
state is matchable**: `:checked`/`:unchecked`, `:disabled`, `:selected`,
`:expanded`/`:collapsed`, `:invalid`, `:busy`, … (the closed set in
`lumen_core::semantics::State`).

```lss
button.primary { background: $primary; color: $bg; border-radius: 6px; }
button.primary:hover { background: #3b82f6ff; }
checkbox:checked { background: #22aa44ff; }
button:disabled { background: #888888ff; }
```

## Themes & tokens (this part is solid)

```lss
@tokens { radius: 6px; }
@theme light { primary: oklch(0.62 0.19 255); bg: #ffffff; }
@theme dark  { primary: oklch(0.72 0.17 255); bg: #101418; }
button.primary { background: $primary; border-radius: $radius; }
```

`$name` resolves theme-scoped first, then `@tokens`. Theme switching
(`set_theme`) rebuilds instantly — the spec's 150 ms color animation is
planned (B.5). Test both themes: `TestApp::with_options(size, theme)`.

## Workflow

1. Attach: `App::new(build).stylesheet(include_str!("../app.lss"))`.
2. Iterate live: `just run-hot <name>` (watches `examples/<name>/app.lss`)
   — edits apply on save; a broken sheet is rejected atomically (old one
   stays live) with `E0101` + span on stderr.
3. **Confirm a rule actually landed** — don't trust your eyes for subtle
   values:
   ```bash
   python3 scripts/agent_client.py call ui.getStyles '{"selector":"#save"}'
   ```
   Values serialize canonically (`{px: 6.0}`, `#3b82f6ff`) with a `source`
   field (only `stylesheet` is reachable today).
4. Golden the result (CPU-exact) per the `verifying-apps` skill.

## Diagnostics reality

- `E0101` parse error + `E0102` unknown property (with did-you-mean) +
  `E0104` unknown token — all fire, with file/line/col spans.
- **`E0103` fires since B.7a**: `opacity: red`, `background: 12px`,
  `display: 4px` etc. are rejected atomically with the expected type
  (applied property set; `$token`/function/list values pass through).
- `border-width:`/`border-color:` are legitimate known properties now
  (B.7a — the spurious E0102 is gone).
- Unknown units are `E0103` with a span (B.7) — `12abc` rejects the sheet.
  Known units: `px % ms s deg fr`; bare numbers still mean px where a
  length is expected.

## References

- `.ai_docs/04-spec-lss-styling.md` — grammar + §10 status table (canonical).
- `docs/plan-remediation-2026-07.md` Phase A.2 / B — what unlocks when.
- `examples/styling`, `examples/iced-parity` — working stylesheets to copy.
- `writing-widgets` skill — LayoutStyle patterns for the layout half.
