# 04 — `.lss` Styling Language (normative)

`.lss` (Lumen Style Sheets) is a typed, CSS-like declarative styling language. It is parsed at app startup, hot-reloaded by the dev server (tier 1), and mirrored 1:1 by a typed Rust `Style` API. Parse/validation failures keep the previous stylesheet live and emit `E0101`/`E0102` diagnostics with spans.

> **⚠ Implementation status (2026-07-09, from the docs↔code audit).** This
> spec describes the target language. The **parser** accepts essentially all
> of it; the **runtime applies a subset**. Styles now resolve **before
> layout** (remediation A.2), so the core layout properties — `display`,
> `flex-direction`, `width`, `height`, `gap`, `padding`, `margin`
> (whole-side) — are real; the rest of the layout set (per-side, flex-*,
> justify/align, min/max, grid tracks, position/inset, overflow) lands with
> Phase B. **Nested `&` rules apply since B.1** (flattened at parse; `& >
> part+` supported), and descendant/`>` combinators now match against the
> real ancestor chain — previously only the rightmost compound was checked,
> so `dialog button` matched every button. **`@media` gates on the live
> window since B.2** (width/height/scale/platform/pointer from the real
> context; resize re-resolves). **State selectors carry the full vocabulary
> since B.6a**: interaction states with CSS-familiar aliases
> (`:hovered`/`:hover`, `:focused`/`:focus`, `:pressed`/`:active`) plus
> every semantic widget state (`:checked`, `:disabled`, `:expanded`, …).
> Still true until the rest of B: `@media container(...)` is a parse error
> (B.2b); `transition:`/`animation:` are **unwired** (no keyframe playback,
> B.5). See §10 for the per-property table. Authoring guidance lives in the
> `styling-lss` skill.

## 1. Grammar (EBNF)

```
stylesheet  := (item)*
item        := rule | tokens_block | theme_block | keyframes | media_block
rule        := selector_list '{' declaration* nested_rule* '}'
selector_list := selector (',' selector)*
selector    := compound (combinator compound)*          // same compound/part grammar as 03 §2,
combinator  := WS | '>'                                 // matched over widget tree (pre-elision)
declaration := property ':' value ('!' 'important')? ';'
nested_rule := '&' part+ '{' declaration* '}'           // nesting: &:hover, &.primary, & > .thumb
tokens_block:= '@tokens' '{' (ident ':' value ';')* '}'
theme_block := '@theme' ('light'|'dark'|'high-contrast') '{' (ident ':' value ';')* '}'
keyframes   := '@keyframes' ident '{' (percent '{' declaration* '}')+ '}'
media_block := '@media' media_query '{' rule* '}'
media_query := '(' ('width'|'height'|'platform'|'pointer'|'scale') (':'|'<'|'>'|'<='|'>=') value ')'
               ('and' media_query)*
value       := literal | '$' ident                      // $token reference
comment     := '//' …EOL | '/*' … '*/'
```

Numbers carry units: `px` (logical pixels, default), `%`, `ms`, `s`, `deg`. Colors: `#rgb/#rrggbb/#rrggbbaa`, `rgb()`, `oklch()`. Strings double-quoted.

## 2. Cascade & specificity
Origin order (low→high): framework defaults < `@theme` < app stylesheets (file order) < inline `.style(...)` from Rust < `!important`. Within an origin, CSS-style specificity `(id, class+state, type)`, ties broken by source order. State parts (`:hover` etc.) match live `NodeFlags`; recomputation on flag change touches only affected nodes.

## 3. Property set v1
(M1 implements all below; names and types binding.)

**Layout** (maps to Taffy): `display` (`flex|grid|none`), `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`, `justify-content`, `align-items`, `align-self`, `align-content`, `gap`, `row-gap`, `column-gap`, `grid-template-columns/rows` (track list: `1fr 200px auto`), `grid-column/row` (`span n`, `a / b`), `width/height/min-*/max-*`, `aspect-ratio`, `padding(-top/right/bottom/left)`, `margin(-…)`, `position` (`relative|absolute`), `inset(-…)`, `overflow` (`visible|hidden|scroll`).

**Visual**: `background` (color|gradient), `border` (`1px solid $border`), `border-(top|…)`, `border-radius` (1–4 values), `shadow` (comma list: `0 2px 8px #0003, inset 0 1px #fff2`), `opacity`, `blend-mode`, `filter` (`blur(4px) brightness(1.1)`), `backdrop-filter`, `clip` (`none|bounds|rounded`), `transform` (`translate() rotate() scale()` — 2D v1), `transform-origin`, `z-index`, `visibility`, `cursor`.

**Typography**: `font-family` (fallback list), `font-size`, `font-weight` (100–900), `font-style`, `font-features` (`"tnum" 1`), `font-variation` (`"wght" 650`), `line-height`, `letter-spacing`, `color`, `text-align`, `text-overflow` (`clip|ellipsis`), `text-wrap` (`wrap|nowrap`), `text-decoration`, `selection-color`.

**Motion**: `transition` (`<prop|all> <dur> <easing> [delay]`, comma list), `animation` (`<keyframes> <dur> <easing> [delay] [count|infinite] [alternate]`), easing: `linear|ease|ease-in|ease-out|ease-in-out|cubic-bezier(…)|spring(stiffness, damping)`.

Animatable properties: all numeric/color/transform/shadow values; layout properties animate via re-layout per frame (document the cost in rustdoc). Reduced-motion: when the OS signals it, durations clamp to 0 unless `animation-force: true`.

## 4. Tokens & themes
```lss
@tokens { spacing-1: 4px; spacing-2: 8px; radius: 6px; font-ui: "Inter", "Noto Sans"; }
@theme light { primary: oklch(0.62 0.19 255); bg: #ffffff; on-bg: #111418; border: #d8dde3; }
@theme dark  { primary: oklch(0.72 0.17 255); bg: #101418; on-bg: #e8ecf1; border: #2a3138; }

button.primary { background: $primary; color: $bg; border-radius: $radius;
  transition: background 120ms ease;
  &:hover { background: oklch(from $primary calc(l + 0.06) c h); }
  &:disabled { opacity: 0.45; }
}
```
`$name` resolves theme-scoped first, then `@tokens`. Theme switching re-resolves tokens and animates color properties over 150 ms by default.

## 5. Widget parts
Built-in widgets expose internal parts as classes documented per widget (02 §10): `slider { } slider .track { } slider .thumb { }`. Custom widgets expose parts by calling `cx.part("thumb")` on the child element.

## 6. Media queries
`width/height` test the **window** by default; `@media container(...)` tests the nearest ancestor marked `.container()`. `platform: windows|macos|linux|android|ios`, `pointer: mouse|touch`, `scale` = DPI factor.

## 7. Computed-value serialization (for `ui.getStyles`)
Every property serializes to JSON as `{ "value": <canonical>, "source": "theme|stylesheet|inline|default", "span": {file,line,col}? }`. Canonical forms: lengths as `{px: f64}`, colors as `#rrggbbaa`, enums as strings. This is API: tests assert against it.

## 8. Rust mirror API
```rust
let s = Style::new().background(theme.primary()).padding(8.0).radius(6.0)
    .transition(Prop::Background, 120.ms(), Easing::Ease);
Button::new("Save").style(s)
```
Every `.lss` property has exactly one corresponding typed setter; the macro test `style_parity!` asserts the sets stay equal (part of M1 DoD).

## 9. Error behavior
Unknown property → `E0102` with Levenshtein did-you-mean; type mismatch → `E0103` with expected type; unknown token → `E0104`. All include file/line/col span. A stylesheet with errors is rejected atomically (old one stays live).

*Status:* E0101/E0102 (did-you-mean)/E0104 + atomic reject + spans are
implemented. **E0103 is never emitted** — type mismatches are silently
ignored (plan B.7). `border-width`/`border-color` are applied but missing
from the known-property list, so they raise a spurious E0102 (plan B.7).
Unknown units are silently treated as unitless (plan B.7).

## 10. Implementation status by property (2026-07-09)

Three levels: **rendered** (visible effect), **applied** (parsed into the
typed style but ignored downstream), **parse-only** (name known, dropped).
Plan tasks: layout → A.2, visual/typography → B.3/B.4, motion → B.5.

| Level | Properties |
|---|---|
| **rendered** | `background` (solid color only), `border` (shorthand width+color), `border-radius` (single value), `backdrop-filter` (blur/saturate + beyond-spec `refraction`/`specular`), `color` (text); **layout (A.2, 2026-07-09):** `display`, `flex-direction`, `width`, `height`, `gap` (both axes), `padding` (whole-side), `margin` (whole-side) — note text-bearing nodes still derive `height` from their glyphs (the text-height rule), and state-part layout rules (`:hovered { width: … }`) relayout via the normal rebuild path |
| **applied, no effect** | `opacity`, `font-size`, `font-weight` (parsed into the typed style; unread by paint/measure — plan B.4) |
| **parse-only** | remaining layout (`flex-wrap/grow/shrink/basis`, `justify-*`, `align-*`, `row/column-gap`, `grid-*` (track lists unparsed), `min/max-*`, `aspect-ratio`, `position`, `inset`, `overflow`, per-side `padding-*`/`margin-*`/`border-*`), background gradients, `shadow`, `blend-mode`, `filter`, `clip`, `transform(-origin)`, `z-index`, `visibility`, `cursor`, `font-family/style/features/variation`, `line-height`, `letter-spacing`, `text-align/overflow/wrap/decoration`, `selection-color`, `transition`, `animation`, `animation-force` |

Runtime constructs status: `@tokens`/`@theme`/`$token` **work**; specificity
+ `!important` **work**; nested `&` rules **applied** (B.1 ✅ — flattened at
parse, incl. `& > part+`); descendant/`>` combinators **match the real
ancestor chain** (B.1 ✅ — the last-compound-only over-match is fixed);
`@media` **gates on the live window** (B.2 ✅ — width/height/scale/
platform/pointer; resize re-resolves); `@media container(...)` **parse
error** (B.2b); relative colors `oklch(from …)`
**unsupported** (B.7); theme-switch animation **missing** (B.5); widget
parts (`slider .track`, `cx.part`) **missing** (B.7); cascade origins other
than the app sheet **unreachable** (B.6); `style_parity!` covers 11
hand-picked properties, not set equality (B.7); `get_styles` serialization
lacks `span` and only reaches the `stylesheet` source (B.7). This section
is deleted when Phase B completes and the spec becomes unconditionally
normative.
