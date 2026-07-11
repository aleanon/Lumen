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
> Still true: `animation:`/`@keyframes` are **unwired** (parse-only —
> see the B.5 amendment in the plan; `transition:` **plays since B.5a**). See §10 for the per-property table. Authoring guidance lives in the
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
Built-in widgets expose internal parts as classes documented per widget (02 §10): `slider { } slider .track { } slider .thumb { }`. Custom widgets expose parts by calling `.part("thumb")` on the child `Element` (the shipped form of the draft's `cx.part(…)` — parts are classes; the ancestor-chain matching from B.1 scopes them to the enclosing widget type).

*Status (B.7):* shipped parts — `slider .track`, `slider .thumb`, `progress .fill`; other built-ins gain parts as they're documented in 02 §10. `Element::part` is public for custom widgets.

## 6. Media queries
`width/height` test the **window** by default; `@media container(...)` tests the nearest ancestor marked `.container()` (an `Element::container()` builder; only `width`/`height` are valid inside `container(…)`, and with no container ancestor the query is false). `platform: windows|macos|linux|android|ios`, `pointer: mouse|touch`, `scale` = DPI factor.

*Status (B.2b):* shipped. The container size is its laid-out size: styles resolve against the previous layout's measurement, then one bounded re-pass per rebuild re-resolves if the fresh layout moved it — so a threshold crossing lands within the same pump; a change caused *by* the re-pass itself waits for the next pump (oscillation guard).

## 7. Computed-value serialization (for `ui.getStyles`)
Every property serializes to JSON as `{ "value": <canonical>, "source": "theme|stylesheet|inline|default", "span": {file,line,col}? }`. Canonical forms: lengths as `{px: f64}`, colors as `#rrggbbaa`, enums as strings. This is API: tests assert against it.

## 8. Rust mirror API
```rust
let s = Style::new().background(theme.primary()).padding(8.0).radius(6.0)
    .transition(Prop::Background, 120.ms(), Easing::Ease);
Button::new("Save").style(s)
```

*Status (B.6b):* the inline tier ships as `.css(Style)` on widgets and
`Element::css` (the draft's `.style(s)` name was already taken by
`LayoutStyle`). Field-wise merge at `Origin::Inline`: beats stylesheet
declarations unless `!important` (§2), participates in the pre-layout merge
(inline layout properties reach taffy), survives the A.5 restyle path, and
works without any stylesheet. `ui.getStyles` reports `source: "inline"` for
scalar/color values; compound fields (gradients, shadows, per-side arrays,
backdrop) apply but are not serialized yet. `transition(…)` setters remain
B.5. The framework-default sheet (`Origin::Default`) is still open —
widget defaults stay hardcoded on the elements.
Every `.lss` property has exactly one corresponding typed setter; the macro test `style_parity!` asserts the sets stay equal (part of M1 DoD).

## 9. Error behavior
Unknown property → `E0102` with Levenshtein did-you-mean; type mismatch → `E0103` with expected type; unknown token → `E0104`. All include file/line/col span. A stylesheet with errors is rejected atomically (old one stays live).

*Status:* E0101/E0102 (did-you-mean)/E0104 + atomic reject + spans are
implemented. **E0103 fires since B.7a** for type mismatches on the applied
property set (color/length/number/keyword expectations; `$token`,
function, and list values pass through) — the sheet is rejected
atomically. `border-width`/`border-color` are in the known-property list
(B.7a). Unknown units are `E0103` with a span since B.7 (known units:
`px % ms s deg fr`; bare numbers stay legal where a length is expected).

## 10. Implementation status by property (2026-07-09)

Three levels: **rendered** (visible effect), **applied** (parsed into the
typed style but ignored downstream), **parse-only** (name known, dropped).
Plan tasks: layout → A.2, visual/typography → B.3/B.4, motion → B.5.

| Level | Properties |
|---|---|
| **rendered** | `background` (solid color, `linear-gradient(<angle>deg, stops…)` — CSS angles, optional `%` positions, Oklab interpolation — and `radial-gradient(stops…)` centered/farthest-corner; conic still unexposed), `border` (shorthand width+color), `border-radius` (1–4 values, CSS expansion `[tl tr br bl]` — the shadow sprite uses the uniform top-left fallback), `shadow` (B.3 — single outer shadow `<dx> <dy> [blur] [spread] <color>`; `inset`/comma lists still unsupported and an `inset` keyword disables the declaration), `visibility` (B.3 — `hidden` removes the subtree from paint, hit-testing, and semantics while keeping its layout space), `clip` (B.3 — `none|bounds|rounded`, overriding the element clip flag; `bounds` squares the corners), `blend-mode` (B.3 — `normal|multiply|screen|overlay|darken|lighten`, subtree compositing layer shared with `opacity`), per-side `border-(top|right|bottom|left)` (B.3 — `<w> <color>` strips on top of the fill; border-radius ignored for per-side strokes), `backdrop-filter` (blur/saturate + beyond-spec `refraction`/`specular`), `color` (text); **layout (A.2, 2026-07-09):** `display`, `flex-direction`, `width`, `height`, `gap` (both axes), `padding`/`margin` (whole-side + per-side longhands `padding-top` … `margin-left`, component-wise override) — note text-bearing nodes still derive `height` from their glyphs (the text-height rule), and state-part layout rules (`:hovered { width: … }`) relayout via the normal rebuild path |
| **applied, no effect** | *(empty since B.4a)* — `font-size`, `font-weight` (synthesized bold on the single face), and `line-height` reach the text stack (measure **and** paint); `opacity` renders since B.3a (subtree compositing layer) |
| **parse-only** | remaining layout (`flex-wrap/grow/shrink/basis`, `justify-*`, `align-*`, `row/column-gap`, `grid-*` (track lists unparsed), `min/max-*`, `aspect-ratio`, `position`, `inset`, `overflow`, ), `filter`, `transform(-origin)`, `z-index`, `cursor`, `font-family/style/features/variation`, `line-height`, `letter-spacing`, `text-align/overflow/wrap/decoration`, `selection-color`, `animation`, `animation-force` |

Runtime constructs status: `@tokens`/`@theme`/`$token` **work**; specificity
+ `!important` **work**; nested `&` rules **applied** (B.1 ✅ — flattened at
parse, incl. `& > part+`); descendant/`>` combinators **match the real
ancestor chain** (B.1 ✅ — the last-compound-only over-match is fixed);
`@media` **gates on the live window** (B.2 ✅ — width/height/scale/
platform/pointer; resize re-resolves); `@media container(...)` **works**
(B.2b ✅ — tests the nearest `.container()` ancestor's laid-out size;
measured post-layout with one bounded re-pass per rebuild, so a size change
is visible within the same pump); relative colors `oklch(from <color|$token> L C H)` **work**
(B.7 ✅ — channel keywords `l`/`c`/`h` + `calc(…)` over `+ - *`,
left-to-right, spaces required around operators; alpha inherited from the
base; `$token`s now resolve inside function args and shorthand lists too);
`transition:` **plays** (B.5a ✅ — paint tier: background/color/opacity/
border-radius interpolate between computed values on nodes with stable
ids, id-keyed so identity survives rebuilds; smooth retarget on
interruption; `delay` honored; reduced motion (`set_reduced_motion`)
snaps; layout-property transitions are documented no-ops; both the
rebuild and the A.5 hover-restyle paths animate); `@keyframes` playback +
the automatic 150 ms theme-switch animation remain **open** (B.5b); widget
parts **work** (B.7 ✅ — `slider .track`/`.thumb`, `progress .fill`;
`Element::part` for custom widgets); cascade origins: **inline works** (B.6b ✅ — `.css(Style)`, see §8);
`Origin::Default` (framework sheet) still unreachable; `style_parity!` asserts **set
equality** over `APPLIED_PROPERTIES` in both directions (B.7 ✅ — every
applied property has exactly one typed setter, every other known property
is provably inert); `get_styles` **carries the
winning declaration's `span`** (`{line, col}` — B.7b ✅) but still only
reaches the `stylesheet` source (origins — B.6b). This section is deleted
when Phase B completes and the spec becomes unconditionally normative.
