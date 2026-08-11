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
> Motion is **wired** (B.5): `transition:` plays since B.5a and
> `animation:`/`@keyframes` play since B.5b (paint tier; see §10). See §10 for the per-property table. Authoring guidance lives in the
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

## 9b. Third-party properties (MOD4, 2026-08-08)

`lumen_style::register_property(name)` adds a property the framework does not
implement. A registered name stops raising `E0102`, and its resolved value is
carried on `Style::custom` (a `BTreeMap`, so serialization stays diffable) for
whoever registered it to read.

It cannot reach `Style`'s built-in fields, and registering over a built-in is
**refused** rather than shadowing it — layout and render consume those fields as
a contract, and an extension able to rewrite them could break invariants the
framework asserts elsewhere. An unregistered name is still `E0102`, so a typo
stays a typo.

This closes the extension point the 2026-08 modularity review ranked in its
top 5: before it, adding a style property meant forking the crate.

## 10. Implementation status by property

*(SD5.1/SD5.2, 2026-08-08: this section is no longer the source of truth and
cannot drift from the code again. The three lists live in the crate —
`KNOWN_PROPERTIES`, `APPLIED_PROPERTIES`, `PARSE_ONLY_PROPERTIES` — and
`crates/lumen-style/tests/property_parity.rs` asserts `KNOWN == APPLIED ∪
PARSE_ONLY`, so a property that parses without an implementation is a **build
failure**. Current split: **78 known, 78 applied, 0 parse-only** — every known `.lss` property now reaches the runtime (PROP1, completed 2026-08-08; the parity test's tripwire ratcheted down with each one and now pins the empty set). A parse-only
declaration now reports `W0107` at parse time instead of silently doing
nothing.)*

## 10b. Historical status by property (2026-07-09)

Three levels: **rendered** (visible effect), **applied** (parsed into the
typed style but ignored downstream), **parse-only** (name known, dropped).
Plan tasks: layout → A.2, visual/typography → B.3/B.4, motion → B.5.

| Level | Properties |
|---|---|
| **rendered** | `background` (solid color, `linear-gradient(<angle>deg, stops…)` — CSS angles, optional `%` positions, Oklab interpolation — and `radial-gradient(stops…)` centered/farthest-corner; conic still unexposed), `border` (shorthand width+color), `border-radius` (1–4 values, CSS expansion `[tl tr br bl]` — the shadow sprite uses the uniform top-left fallback), `shadow` (B.3 — single outer shadow `<dx> <dy> [blur] [spread] <color>`; `inset`/comma lists still unsupported and an `inset` keyword disables the declaration), `visibility` (B.3 — `hidden` removes the subtree from paint, hit-testing, and semantics while keeping its layout space), `clip` (B.3 — `none|bounds|rounded`, overriding the element clip flag; `bounds` squares the corners), `blend-mode` (B.3 — `normal|multiply|screen|overlay|darken|lighten`, subtree compositing layer shared with `opacity`), per-side `border-(top|right|bottom|left)` (B.3 — `<w> <color>` strips on top of the fill; border-radius ignored for per-side strokes), `backdrop-filter` (blur/saturate + beyond-spec `refraction`/`specular`), `color` (text); **layout (A.2, 2026-07-09):** `display`, `flex-direction`, `width`, `height`, `gap` (both axes), `padding`/`margin` (whole-side + per-side longhands `padding-top` … `margin-left`, component-wise override) — note a text-bearing node derives its size from its glyphs only where nothing is declared: an explicit `width`/`height` on a label is applied (amended 2026-08-12; it was previously overwritten by the measurement), and the run paints at the box's top-left. State-part layout rules (`:hovered { width: … }`) relayout via the normal rebuild path |
| **applied, no effect** | *(empty since B.4a)* — `font-size`, `font-weight` (synthesized bold on the single face), and `line-height` reach the text stack (measure **and** paint); `opacity` renders since B.3a (subtree compositing layer) |
**The value-level hole is mostly closed** (SD5.x, 2026-08-08). `W0107` reports
an unimplemented *property*; **`W0109` now reports an unusable value on an
implemented one** — `text-align: justify`, `overflow: scroll`, `display: flext`
were all silent before. Raised once per declaration at parse time, like W0107.

The check judges **bare keywords, and numbers on single-scalar properties**
(`SCALAR_PROPERTIES`: `aspect-ratio`, `opacity`, `flex-grow`, `flex-shrink`,
`font-weight`, `line-height`). A keyword either is in a property's accepted set
or is not; a number is judged only where the number is the *whole* value.

Compound values are still not judged, and the list is opt-in rather than
"everything that is not a shorthand", because the failure modes are not
symmetric: **a missing entry costs a warning that does not fire; a wrongly
included compound property costs a warning on a stylesheet that works.**
`transition: 120ms` is the case that proves it — a duration with no property or
easing, which applies nothing and is nonetheless legal input the general form of
the check flagged.

Note `get_styles` does *not* answer this question: it reports the **declared**
value and its source span, not what was applied, so a rejected value still
appears there. `ui.explain {kind: "style"}` distinguishes them per node.

### PROP1 is complete: 29 parse-only → 0 (2026-08-08)

Every known `.lss` property now reaches the runtime. What the work actually
found, since the estimate was wrong in both directions:

* **Nine were pure bridges** — the capability existed and only the wiring was
  missing (`overflow`→clip, `text-decoration`→rect drawing, `cursor`→hover +
  winit, `selection-color`→a hardcoded literal, `text-wrap`→`wrap_width`,
  `transform`→`PushLayer`'s always-`IDENTITY` `Affine`, `letter-spacing` and
  `font-family`→existing `TextStyle` fields, `text-align`→a hardcoded
  `TextAlign::Start` at nine call sites).
* **`filter: blur()` needed a real render pass** and got one: `PushLayer` gained
  `filter_blur`, the CPU backend blurs the layer pixmap, the GPU backend reuses
  a `blur_texture` helper extracted from the backdrop path. That extraction was
  verified by **byte-identity** of the `backdrop_glass` hash, because the
  parity suites are tolerance-based and would have accepted a subtly wrong blur.
* **`z-index` needed the right sort, not stacking contexts.** A flat z sort
  breaks the depth-keyed clip stack; sorting each node's CHILDREN during the
  preorder walk does not, and matches CSS's stacking-context scoping.
* **`text-overflow` needed a display/semantic split** — `NodeMeta.display_text`
  is painted while `content` keeps the full string, so `ui.getTree` never
  reports `"Some long lab…"`.
* **`font-variation` is applied but inert with the bundled face**, which is
  static. Verified against a registered variable face (Ubuntu Sans Mono `wght`):
  `"wght" 800` rendered 2212 ink px against 751 for `"wght" 100`. Not gated in
  CI, because ADR-005 forbids depending on a system font.

The remaining limits are per-property and documented at each parse helper:
`filter` is blur-only, `z-index` is non-negative, `transform` rejects percentage
translate, `text-align` rejects `justify`, `overflow` rejects `scroll`/`auto`.
Each rejects rather than silently approximating, and reports `W0109`.


**`text-overflow: ellipsis` needed a display/semantic split, and got one** (2026-08-08). The obstacle was real, and is worth recording because
`TextEngine::layout_ellipsized` exists and looks like it should just be wired up
(2026-08-08). the paint path shapes the node's own text, so rendering an ellipsis means
painting a *different string* from the one the node carries — while the semantic
tree, the agent and assistive tech must keep reading the **full** text.
Truncating the stored string would make `ui.getTree` report "Some long lab…",
a worse defect than the property not working.

Resolved by giving `NodeMeta` a `display_text` the paint pass prefers, leaving
`content` (and therefore semantics) untouched — one binding in the paint loop.
`TextEngineApi::ellipsized_text` returns the *string* rather than a block, and is
a **provided** trait method expressed in terms of `layout`, so the MOD3 seam
stays as narrow as it was and every implementor gets it free. A test asserts the
semantic label is still the full string; that assertion is the property's point.

**`z-index` works by sorting SIBLINGS, which is what CSS means** (2026-08-08).
The earlier objection was correct about the flat approach: `Tree::z_order()`
sorts the whole document order, and the paint pass tracks clip layers **by
depth**, so a flat sort breaks clip nesting. `Tree::paint_order()` instead sorts
each node's children during the preorder walk — parents still precede children,
subtrees stay contiguous, and the clip stack is untouched.

That is also the right semantics rather than a compromise: CSS scopes `z-index`
to a stacking context, and the parent is the context here, so a high-`z` child
does not escape a low-`z` ancestor — which is CSS behaviour too. Equal `z` keeps
document order, so a tree with no `z-index` produces exactly the previous list
(asserted).

**Non-negative only.** `Tree`'s `z` is a `u32` with `0` as the default, leaving
no room below an unstyled sibling; a negative value needs that field widened,
which touches hit-test ordering and `OVERLAY_Z`. Rejected with `W0109` rather
than clamped, since clamping would make `z-index: -1` look like it worked.
Overlay roots keep `OVERLAY_Z`: they route to the overlay pass regardless, and a
stylesheet must not be able to demote a dropdown under the page.

| **parse-only** | *(none)* |

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
rebuild and the A.5 hover-restyle paths animate); `@keyframes`/`animation:` **play**
(B.5b ✅ — paint tier sampled per phase with per-segment easing; iteration
count/`infinite`/`alternate`/delay honored; **fills forwards** on
completion — a deliberate deviation from CSS's default snap-back;
`animation-force: true` keeps playing under reduced motion); theme
switching **animates colors over 150 ms** (B.5b ✅ — implicit
background/color transition seeded from the old theme's computed values on
id-bearing css-styled nodes; suppressed by reduced motion); widget
parts **work** (B.7 ✅ — `slider .track`/`.thumb`, `progress .fill`;
`Element::part` for custom widgets); cascade origins: **inline works** (B.6b ✅ — `.css(Style)`, see §8);
`Origin::Default` (framework sheet) still unreachable; `style_parity!` asserts **set
equality** over `APPLIED_PROPERTIES` in both directions (B.7 ✅ — every
applied property has exactly one typed setter, every other known property
is provably inert); `get_styles` **carries the
winning declaration's `span`** (`{line, col}` — B.7b ✅) but still only
reaches the `stylesheet` source (origins — B.6b). This section is deleted
when Phase B completes and the spec becomes unconditionally normative.
