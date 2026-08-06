# Lumen consumer API — adversarial review

Scope: the API surface an application developer (human or AI) writes against —
`crates/lumen/` (the facade), `crates/lumen-widgets/`, signals/reactivity in
`crates/lumen-core`, the `.lss` styling split, ~50 example crates, and the six
`.claude/skills/` documents that exist to compensate for what the API doesn't
say for itself. Every claim below is grounded in source read during this
review, cited as `path:line`. Docs were read but not trusted; several are
shown below to be stale, which is itself a finding (§9).

Reviewer note on method: reading was split across parallel research passes
(facade/type design/errors, examples corpus, widget API, state/reactivity,
styling, skills) plus direct spot-checks by the author of this report to
corroborate or extend each pass. Where two passes independently reached the
same conclusion by different methods (e.g. the facade-bypass count, the
signal-type-mismatch panic, the initial-stylesheet silent failure), both are
cited — this is noted inline as corroboration, not padding. One correction
worth stating up front: an initial exhaustive-panic sweep of
`lumen-widgets`/`lumen-core` found ~248 raw `.unwrap()/.expect(/panic!/
assert!/unreachable!` hits, but a second, more careful pass confirmed all
but 16 sit inside `#[cfg(test)]` blocks — the production-path widget layer
(`text_input.rs`, `slider.rs`, `check_box.rs`, `combobox.rs`, `pick_list.rs`,
`grid.rs`, `forms.rs`, `nav.rs`, `undo.rs`) does **not** panic on bad app
input. This review's panic-related findings (§ Finding 3) are drawn from
that narrower, verified set, not the raw grep count. No `cargo build
--workspace` or `cargo test --workspace` was run; disk headroom was
preserved throughout.

---

## Verdict

Lumen's consumer API has one real, structural advantage over iced/egui: there
is no `Message` enum, no `impl Application for MyApp`, and no
generic-over-`Message` widget tree to thread through every function signature
— `App::new(impl Fn(&mut BuildCx) -> Element + 'static)` plus
`cx.signal(key, init)` gets a working reactive app with less ceremony than any
of the frameworks it's benchmarked against in this review. The agent/testing
surface (`ui.getTree`, `ui.lint`, `assert_view_coherent`, stable selectors) is
genuinely something none of iced/egui/Dioxus/Slint/Flutter has an equivalent
of, and it is the framework's real differentiator.

But the day-to-day widget-construction and styling surfaces are inconsistent
in ways that matter specifically because the primary consumer cannot see the
screen. The project's own binding rule ("every widget is a typed struct,"
`.claude/skills/writing-widgets/SKILL.md:14-15`) is violated by the six
highest-traffic widget constructors in the codebase — `widgets::button` alone
outnumbers `Button::new` 117:22 in the corpus — and the two code paths for
"a button" are independently-maintained and visibly different (different
corner radius, font weight, padding). The `.lss` parser accepts 89 properties
and the runtime applies 37 of them, with no diagnostic marking the other 39 as
"parsed but ignored." Handler-staleness protection (a real, working
compile-time check, `stable_handler!`) exists but is opt-in, so the one
mechanism built to prevent "click does nothing" bugs is absent from every
widget constructor's type signature. None of this is exotic Rust-API-design
malpractice — every example is a small, mechanical fix — but collectively it
means an AI that trusts the compiler and the example corpus (its two most
plausible sources of truth, absent a human eyeballing a screenshot) will
reliably reproduce every one of these defects.

**API quality: C+.** Real strengths in the entry point and the reactive core;
real, fixable rot in the widget-construction and styling middle layers, plus
four confirmed instances of the project's own skills/docs being stale about
exactly the areas this review covers — a documented, binding process
(`AGENT.md` "Doc currency") not being followed for its own hardest cases.

**"Genuinely easier for an AI to write than iced/egui": B-.** Yes for
*structure* (no Message enum, Copy-signal reactivity, a real
agent-introspection API with stable selectors and a coherence oracle that
iced/egui/Dioxus/Slint/Flutter have no equivalent of). Not yet for *safety
net*: the silent-no-op failure mode — a mistake that compiles, runs, and
produces literally no observable difference — is exactly the failure mode an
AI is worst equipped to catch, and this review found it is Lumen's single
most common failure mode, concentrated in styling and widget construction.

---

## Scorecard

| Area | Rating | Justification |
|---|---|---|
| Facade (`crates/lumen`) | **Weak** | Works for toy apps (`examples/hello`); 45/51 example crates bypass it by design (ADR-W2, `.ai_docs/07-decision-log.md:447`), and it's missing re-exports (`theme`, `element::Shadow`) even a facade-committed app needs. |
| App/BuildCx entry point | **Strong** | `App::new(impl Fn(&mut BuildCx) -> Element + 'static)` — no `Message` type anywhere in the framework (zero grep matches), no trait impl, one elided lifetime, `Element` itself is flat/concrete/non-generic (not `Element<Msg, Renderer, Theme>`). Genuinely simpler than iced/Elm-family APIs. |
| Signals/reactivity core | **Adequate** | `Signal<T>` is a clean, hand-written `Copy` handle (deliberately not `#[derive(Clone)]`, to avoid a spurious `T: Clone` bound leaking onto every signal); `State` is a costless blanket impl and doesn't require `Clone` (only `.get()` does — `.with()`/`.update()` work on non-`Clone` resources). But `cx.signal`'s key↔type link is unchecked (runtime panic or silent aliasing) and `cx.scope` memoization is silently opt-in with no interactive lint. |
| Widget construction API | **Broken** | 4 coexisting construction shapes; the binding "typed struct" migration claim is false for the highest-traffic widget (`button`); two divergent implementations ship real bugs (stale slider format, missing checkbox tick, permanently inert menu shim). Builder conventions *within* the typed-struct family are genuinely consistent (`impl_common!`, 57 invocations across 30 files) — the inconsistency is specifically the free-function/typed-struct split, not the typed structs themselves. |
| Styling (`.lss` ↔ `LayoutStyle`/`Style`) | **Weak** | 39 of 89 parser-known properties are silent no-ops by design; a further silent gap on *applied* properties (a misspelled keyword value, e.g. `display: flext;`, is accepted by `E0103`'s shape check and then silently unset by `apply()`); the diagnostic system covers syntax errors well but has no tier for "recognized, never applied" or "recognized, malformed value accepted." |
| Error handling & diagnostics | **Adequate** | A real `Diagnostic`/`E####`/`W####` system exists with genuinely good cases (`E0102` did-you-mean, spanned `E0101` recovery, `ResolveError::{NotFound{nearest}, Ambiguous{candidates}}` on selector lookups) and the proc-macro diagnostics are a standout (`stable_handler!`'s error is deliberately shaped, per its own code comment, "so the unsatisfied-bound error reads as a handler-currency violation, pointed at the offending closure," backed by a `compile_fail` doctest). But coverage is uneven: the one confirmed hard-panic footgun (signal type mismatch) has a poor message, most mistakes silently no-op, and the two real lints that exist for exactly this (`W0001` duplicate id, `W0106` unimplemented action) are opt-in — neither `lumen-shell` nor `lumen-test` calls `lint()`/`audit_actions()` anywhere, so a plain `cargo run` or ordinary test gives zero signal on either. |
| Type design (invariants in types) | **Adequate** | `LeafWidget`/`NodeContent` enum make illegal states (text+image+canvas at once) genuinely unrepresentable; `impl_common!` gives a uniform base; the widget layer itself does not panic on bad app input (a full sweep found only 16 production-path panics in `lumen-widgets`/`lumen-core` combined, nearly all in the type-erased signal store). But `Action`↔handler pairing, id charset, `elide_semantics`, and `Element`'s `scope_key`/`shared` pairing remain stringly-typed/boolean/doc-comment-only footguns. |
| Async/data seam (ADR-M2) | **Strong** | Bring-your-own-transport (`Resource<T,E>`, `Sink`) keeps HTTP out of core; `Runtime`'s `!Send` shape genuinely prevents the worst cross-thread misuse at compile time, not just by convention. |
| Skills as compensating docs | **Weak signal for the API** | 32 catalogued warnings across 6 skills; a large fraction name a concretely fixable API gap (missing `Copy` bound, missing id validation, missing action/handler pairing) rather than inherent domain knowledge. |
| Live-agent/verification surface | **Adequate** | Powerful and unique — no competitor has an equivalent — but its own skill catalogs "traps that have burned an agent," including a documented history of unreadable ambiguous-selector errors. |

---

## Code-in-anger walkthrough

`examples/counter/src/lib.rs` (full file read; not the `iced-parity` toy
version — the standalone example, which exercises signals, dual windows,
native menus, and OS notifications). Annotated by how non-obvious each
requirement is to a newcomer — human or AI — ranked most-to-least surprising.

```rust
use lumen_core::state::Runtime;
use lumen_widgets::element::Shadow;                              // (1)
use lumen_widgets::system::{MenuItem, MenuModel};
use lumen_widgets::{widgets, App, BuildCx, Element};
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle};

pub fn main_app() -> App {
    App::new(build)
        .stylesheet(include_str!("../app.lss"))
        .window(                                                  // (2)
            lumen_widgets::system::WindowDesc { id: "stats".into(), .. },
            stats_window,
        )
}

fn stats_window(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i64);                      // (3)
    ...
}

fn build(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i64);                      // (3, again)
    let v = count.get(cx.runtime());
    let step = move |n: i64| move |rt: &Runtime| count.update(rt, |c| *c += n);

    cx.register_command("tally.inc", step(1));                    // (4)
    cx.set_menu(MenuModel { items: vec![MenuItem::submenu("tally", "Tally",
        vec![MenuItem::new("tally.inc", "Increment").accel("Ctrl+I"), ...])] });

    let mut card = widgets::column(vec![                          // (5)
        txt("TALLY", 13.0, 700.0).class("caption"),
        button("+1", "accent", step(1)).id("inc"),                // (6)
    ]).id("card");
    card.style.align_items = Some(Align::Center);                 // (7)

    Element { role: ..., style: LayoutStyle { .. }, children: vec![card],
              ..Element::default() }.id("page")                   // (8)
}
```
(elided for brevity; full text at `examples/counter/src/lib.rs:1-175`)

Ranked, most non-obvious first:

1. **`use lumen_widgets::element::Shadow;`** (`lib.rs:7`) — `Shadow` is not
   re-exported by the facade at all (`crates/lumen/src/lib.rs:36-39` only
   lists `a11y, forms, i18n, nav, system, undo, widgets, widgets_extra,
   widgets_m1, widgets_m3, widgets_m4`). An author following the facade-only
   discipline the project's own docs describe (`docs/api-audit-1.0.md:1-6`)
   hits a hard wall on drop shadows, one of the first things any card-style
   layout wants. Nothing in the error points at "use lumen-widgets directly
   instead" — it's just an unresolved-import compiler error against `lumen::`.

2. **Two windows share state by re-using the string key `"count"`**
   (`lib.rs:32`, `:86`) — this *is* the cross-window sync mechanism, and it
   works, but nothing ties the two `cx.signal("count", || 0i64)` call sites
   together at compile time. A typo in either, or a type mismatch (`0i64` vs
   `0i32`), either silently desyncs the windows or panics with
   `"signal type mismatch"` (`crates/lumen-core/src/state.rs:953,1102`) —
   see Finding 3.

3. **`cx.signal("count", || 0i64)` runs on every `build`, but `init` only
   fires once** — this is documented in `writing-widgets/SKILL.md:47-50` but
   not visible from the call site itself; there is no `SignalHandle::created`
   flag or equivalent to tell you whether this call created the slot or found
   an existing one.

4. **`cx.register_command("tally.inc", step(1))` and
   `MenuItem::new("tally.inc", ...)` must use the identical string** — two
   independently-typed string literals, no shared constant, no compiler check
   linking them. A typo in either means a menu item that looks fully wired
   (label, accelerator, submenu placement all correct) but silently does
   nothing when clicked.

5. **`widgets::column(vec![...])` returns a free-function `Element`, not a
   typed struct** — this is the *dominant* style in the codebase (117:22
   over `Button::new`, per the widget-API research pass) despite the
   project's binding rule that "every widget is a typed struct... never
   write a bare `pub fn … -> Element`" (`writing-widgets/SKILL.md:14-15`).
   An author reading this file as a template — which `lumen new` itself
   scaffolds this exact style, `crates/lumen-cli/src/main.rs:396-397` —
   learns the style the project's own skill says not to write.

6. **`button("+1", "accent", step(1))`** is a *local helper* defined at
   `lib.rs:69-76`, not a framework widget — it wraps `widgets::button` to add
   font-size/weight/padding, because the base free-function widget API
   doesn't offer chained styling ergonomics the way the typed `Button`
   struct's `.primary()`/`.ghost()` methods do. Nearly every example in this
   review reinvents a version of this helper (see Findings, examples pass).

7. **`card.style.align_items = Some(Align::Center);`** — a raw field write on
   `LayoutStyle`, not a builder method. Mixing `.lss` classes (`.class("btn")`)
   and direct Rust field writes in the same widget, with no rule for which
   goes where beyond convention (confirmed in the styling research pass:
   there's no type-level guidance, only the cascade order documented in a
   code comment at `crates/lumen-widgets/src/app.rs:4345`).

8. **`Element { role: ..., ..Element::default() }.id("page")`** at the root
   — building the outermost node means knowing `Element`'s ~40-field literal
   syntax and its `Default` impl directly, because a page-level container
   with custom role + full-bleed layout has no typed-struct or free-function
   shortcut of its own; you drop out of both the free-function and
   typed-struct worlds back to the raw struct.

---

## Findings

Numbered by severity. Each has evidence, consumer-facing consequence, and a
concrete fix.

### Critical

**1. `widgets::button`/`checkbox`/`slider`/`scroll`/`text_field_basic`/`progress_bar`
are independently-maintained, behaviorally divergent duplicates of the typed
structs the project's binding rule says they were migrated into — and two of
the six ship regressions relative to the typed sibling.**

Evidence:
- `crates/lumen-widgets/src/element.rs:353-374` (`Element::button`): `corner_radius: 6.0`, `font_size: 16.0`, `weight: 400.0`, `padding: Edges::all(Dim::px(8.0))`.
- `crates/lumen-widgets/src/button.rs:42-74` (`Button::new`): `corner_radius: 8.0`, `font_size: 15.0`, `weight: 600.0`, asymmetric `padding` (16px h / 9px v).
- `crates/lumen-widgets/src/widgets.rs:155-160` — the free function calls `Element::button(label)`, **not** `Button::new(label).into()`.
- `.claude/skills/writing-widgets/SKILL.md:21-24`: *"The legacy fn-style modules … were migrated to typed structs on 2026-07-20 — each `pub fn foo(...) -> Element` now has a same-named `Foo` struct + a thin `fn` shim … kept for source compatibility."* — true for 26 of 32 wrapped functions, false for these 6.
- `widgets.rs:247`: `value: Some(format!("{v:.0}"))` on `slider()` — the exact stale-formatting bug `slider.rs:44-46` documents as *fixed* in the typed `Slider` ("The old fixed `{:.0}` made a `0.0..1.0` slider report `"0"` at every position — the agent and assistive tech saw a control that never changed").
- `widgets.rs:163-201` (`checkbox`) has no tick-mark element at all; `check_box.rs:39-49` draws one.
- `widgets_extra.rs:317-319` shims `menu(items)` to `Menu::new(items).into()` with **no way to pass `Menu::on_select`** — per the struct's own doc (`widgets_extra.rs:258-261`), "without it the items are inert." Every call site using the free function gets a permanently inert menu.
- Usage skew confirms this is not dead legacy code: `widgets::button(` appears in **117** call sites across `tests/`/`examples/` vs. **22** for `Button::new(`.

Consequence: an app mixing the two styles — which the example corpus does
constantly, since `typed_form` is the *only* place the typed API is used
organically — gets visibly inconsistent buttons, a checkbox that never shows
a check, a value slider that always reports "0" to the agent/screen reader
regardless of position, and a menu that silently never fires. None of this
raises a compiler warning, a lint, or a runtime diagnostic.

Fix:
```rust
// crates/lumen-widgets/src/widgets.rs — before
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element {
    Element::button(label).on_click(on_click)
}

// after — match the 26 already-correct shims
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + Copy + 'static) -> Element {
    Button::new(label).on_press(on_click).into()
}
```
Apply the same fix to `checkbox`, `slider`, `scroll`, `text_field_basic`,
`progress_bar`; delete `Element::button`/`Element::checkbox` or mark them
`#[deprecated]`.

---

**2. 39 of the `.lss` parser's 89 known properties are silently accepted and
never applied, with no diagnostic tier distinguishing "recognized but not
implemented" from "recognized and working."**

Evidence:
- `crates/lumen-style/src/properties.rs:4-89` — `KNOWN_PROPERTIES`, 89 names.
- `crates/lumen-style/src/style.rs:371-409` — `APPLIED_PROPERTIES`, 37 names.
- `crates/lumen-style/src/style.rs:413-503` — `apply()`, a `match` ending in a bare `_ => {}` (line 502) that silently discards every declaration whose property isn't one of the ~30 real arms.
- Diff set (parses clean, zero effect, zero diagnostic): `justify-content`,
  `align-items`, `align-self`, `align-content`, `flex-grow/shrink/basis/wrap`,
  `min/max-width/height`, `aspect-ratio`, `grid-template-columns/rows`,
  `grid-column/row`, `position`, `inset(+sides)`, `overflow`, `filter`,
  `transform`, `transform-origin`, `z-index`, `cursor`, `font-family`,
  `font-style`, `font-features`, `font-variation`, `letter-spacing`,
  `text-align`, `text-overflow`, `text-wrap`, `text-decoration`,
  `selection-color`.
- Contrast: a genuinely misspelled/unknown property *does* hard-error —
  `crates/lumen-style/src/parser.rs:512-519` emits `E0102` with a
  Levenshtein did-you-mean, and the whole sheet is atomically rejected
  (`crates/lumen-widgets/src/app.rs:344-348`). The failure mode for a typo
  is loud; the failure mode for a real, spelled-correctly, unwired property
  is completely silent.
- Two further silent-no-op edges inside the *working* subset: `shadow: ...
  inset` (or comma-separated lists) parses as a shadow declaration and
  produces **no shadow at all** (`.claude/skills/styling-lss/SKILL.md:50`);
  `transition`/`animation` on a node with no stable `.id(...)` parses,
  matches, and the values simply **snap** instead of animating
  (`styling-lss/SKILL.md:73-82`) — neither case is distinguished from
  "working" by any diagnostic.

Consequence: this is the single worst failure mode for a framework whose
premise is that an AI consumer cannot see the screen. `styling-lss/SKILL.md`
documents the *current* gap accurately (verified against source, no drift
found) — but nothing in the *code* prevents the gap from growing silently:
adding a new `KNOWN_PROPERTIES` entry without a matching `apply()` arm
produces a regression with no failing test, because `style_parity!`
(`crates/lumen-style/tests/style.rs:21-146`) only tests the *applied* set's
round-trip, not that every known property either has an arm or an explicit
"parse-only" allow-list entry.

Fix: add a `PARSE_ONLY_PROPERTIES` allow-list next to `KNOWN_PROPERTIES`;
extend the `style_parity!` test to assert
`KNOWN_PROPERTIES == APPLIED_PROPERTIES ∪ PARSE_ONLY_PROPERTIES` (fails the
build the moment a property falls into neither bucket); emit a new
diagnostic tier for declarations matching the parse-only set:
```rust
// crates/lumen-style/src/style.rs — apply(), before
_ => {}

// after
p if PARSE_ONLY_PROPERTIES.contains(&p) => diagnostics.push(
    Diagnostic::warn(codes::W0105,
        format!("`{p}` is recognized but not applied by this runtime (parse-only)"),
        decl.span)),
_ => {}   // now genuinely unreachable for any *known* property
```

---

**3. Signal identity has no compile-time link to its type — same-key,
different-`T` panics with an unhelpful message; same-key-typo silently
aliases or desyncs state.**

Evidence:
- `crates/lumen-widgets/src/element.rs:686`: `pub fn signal<T: State, K: Hash + Debug>(&self, key: K, init: impl FnOnce() -> T) -> Signal<T>` — `T` is inferred independently at every call site; nothing associates a given key with a fixed type.
- `crates/lumen-core/src/state.rs:949-953` (`get`) and `:1097-1102` (`update`): `.expect("signal slot missing")`, `.expect("signal type mismatch")` — generic panics naming neither the key nor the two types that disagreed.
- The framework's own examples create the exact shape that triggers this:
  `examples/counter/src/lib.rs:32,86` and `examples/multi_window/src/lib.rs`
  both share state across two independently-built window roots purely by
  re-using a string key.
- Key-reuse-without-type-mismatch is *not* a panic — it's an intentional
  re-attach (`Runtime::signal_at`, tested at `state.rs:1433-1441`,
  "re-creating the same key keeps the existing value") — so an accidental
  key collision at two logically distinct call sites (e.g. an LLM generating
  a list where two rows compute the same key by a copy-paste-index bug) is
  indistinguishable from the intended "shared window state" pattern: no
  error at all, just silently aliased state.

Consequence: the same API shape produces two opposite failure modes
depending on whether the accidental collision also disagrees on `T` — an
opaque panic in one case, silent data corruption in the other — and neither
is something a static type check would have caught, because `cx.signal`'s
signature doesn't require the caller to prove the key was declared once.

Fix (message-only, zero breakage — and cheap: the human-readable key is
already tracked three lines away and simply not threaded into the panic,
`crates/lumen-core/src/state.rs:611`):
```rust
.expect("signal type mismatch")
// →
.unwrap_or_else(|| panic!(
    "signal {key:?}: stored as {stored_ty}, read as {}",
    std::any::type_name::<T>()))
```
Fix (structural, medium breakage): fold `TypeId::of::<T>()` into the slot's
identity hash (today it's `hash_id(&key)` only, `state.rs:586`, deliberately
*not* keyed on `T`) so a same-key-different-type collision can't alias a
slot in the first place; or a `SignalKey<T>` newtype declared once per
logical piece of state and passed by value to every `cx.signal` call site
that means to refer to it, closing both the type-mismatch-panic and the
silent-key-collision holes at once.

### High

**4. The facade is bypassed by 45 of 51 example crates — by design (ADR-W2)
— which means the example corpus, the most plausible source of few-shot
grounding for an AI, teaches an import style contradicted by the project's
own scaffold and semver-freeze documentation.**

Evidence:
- `crates/lumen/src/lib.rs:1-5`: *"User code and examples depend only on
  `lumen` (and `lumen-test`); nothing imports the internal crates
  directly (02 §11)."*
- `.ai_docs/07-decision-log.md:447` (ADR-W2, 2026-07-10): *"in-repo direct
  crate imports are blessed; scaffolded apps are facade-only. The 91 in-repo
  files importing `lumen_widgets`/`lumen_core`/… directly are **not**
  migrated to the `lumen` facade… The facade rule narrows to the
  user-facing surface: `lumen new` scaffolds depend on `lumen` +
  `lumen-test` only."*
- Direct audit of all 51 `examples/*/Cargo.toml`: only `hello`,
  `hello_android`, `hello_ios`, `hello_web`, `settings_android` depend on
  `lumen` (the facade); the other 45-46, including every "real app" example
  named in this review's brief (`counter`, `todos`, `typed_form`,
  `datagrid`, `pokedex`, `multi_window`, `download_progress`, `gallery`,
  `iced-parity`), depend directly on 2-6 internal crates. A second,
  independent pass over the same corpus (file-level rather than
  crate-level) counted 47 crates with a direct internal-crate dependency
  and 5 source files importing via `use lumen::` — same conclusion, minor
  counting-method variance. Neither pass found a `prelude` module anywhere
  in `lumen`/`lumen-widgets`; every import is an explicit path, never a glob.
- `crates/lumen-cli/src/main.rs:396-397` confirms `lumen new` really does
  scaffold `use lumen::widgets::{button, column, text}; use lumen::App;`.
- `docs/api-audit-1.0.md:26-30`: *"This snapshot is the **1.0 public API
  baseline**. `cargo-semver-checks` gates every subsequent release against
  it"* — this guarantee covers only the facade; the internal crates 45+ of
  51 examples depend on carry no such promise.
- The facade is also missing real re-exports several examples need: `theme`
  (used by every `iced-parity` screen) and `element::Shadow` (used by
  `counter`, `todos`, `typed_form`) are not in the re-export list at
  `crates/lumen/src/lib.rs:36-39` (confirmed independently: zero hits for
  `Shadow`/`element` in that file). This is a real, sharp-edged gap:
  `Element::shadow(mut self, shadow: Shadow) -> Self` (`element.rs:457`) is
  a public builder method on the very type the facade *does* export — a
  facade-only app can call `.shadow(...)` but has no way to name the
  argument type it must construct. 26 example source files reference
  `Shadow`.

Consequence: an AI that reads the example corpus to learn "how do I write a
Lumen app" — which is exactly what this review's own instructions did —
learns the internal-crate-import style, not the facade style `lumen new`
scaffolds and the semver-freeze document covers. Code produced this way
compiles and runs, but is not covered by the 1.0 API stability contract and
could break on an internal-crate refactor that the facade would have
absorbed.

Fix: two independent, non-exclusive options. (a) Re-export the gaps
(`theme`, `element::Shadow`, and audit the remaining ~23 un-re-exported
`pub mod`s in `lumen-widgets` for others) so a facade-only app is actually
viable for the patterns the examples exercise. (b) Regenerate the example
corpus's `Cargo.toml` + imports to route through `lumen::`, matching what
`lumen new` already recommends, so the majority of AI-visible reference code
teaches one consistent, frozen-API pattern.

---

**5. Handler-staleness protection (ADR-013) is an opt-in macro, not a bound
on the widget constructors — so the one compile-time safety net built for
"click does nothing" is absent by default everywhere.**

Evidence:
- `crates/lumen-widgets/src/widgets.rs:155-158`: `pub fn button(label: impl
  Into<String>, on_click: impl Fn(&lumen_core::Runtime) + 'static) ->
  Element` — no `Copy` bound. Same shape on `Button::on_press`
  (`button.rs:77`) and every other `on_*` setter surveyed.
- `crates/lumen-macros/src/lib.rs:35-56` — `stable_handler!` exists
  precisely to force a compile error when a closure captures non-`Copy`
  (i.e. owned, stale-able) state, but it's a separate macro the author must
  remember to wrap the closure in.
- Neither `building-apps/SKILL.md`'s own canonical example (`move |rt|
  count.update(rt, |c| *c -= 1)`, unwrapped) nor `writing-widgets/SKILL.md`'s
  own canonical widget template (`Rc::new(move |rt| on.update(rt, |b| *b =
  !*b))`, also unwrapped) uses the macro — the guidance is only "when in
  doubt" (`writing-widgets/SKILL.md:182`), and `debugging-lumen/SKILL.md:16`
  uses it only *after the fact*, as a diagnostic technique once a bug is
  already suspected.

Consequence: the mechanism the project already built for this exact bug
class (documented as a named, recurring symptom in `debugging-lumen`) is
never exercised by ordinary widget construction, so it protects nothing
unless the app author independently remembers a separate incantation.

Fix:
```rust
// before
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element

// after
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + Copy + 'static) -> Element
```
Apply across every `on_*` parameter in `widgets.rs` and every typed struct's
setter; this is what `stable_handler!` already checks, made unconditional.

---

**6. Initial-load `.lss` syntax errors are completely silent — the app
renders fully unstyled with zero diagnostic — in contrast to the hot-reload
path, which does log.**

Evidence:
- `crates/lumen-widgets/src/app.rs:223,344-348`:
  ```rust
  app_sheet: self.stylesheet.as_deref().and_then(parse_sheet),
  fn parse_sheet(src: &str) -> Option<Stylesheet> {
      let (sheet, diags) = lumen_style::parse("app.lss", src);
      (!lumen_style::has_errors(&diags)).then_some(sheet)
  }
  ```
  No `eprintln!`, no `rt.log`, nothing — `app_sheet` just becomes `None`.
- Contrast `set_stylesheet` (the hot-reload path,
  `crates/lumen-widgets/src/app.rs:2872-2890`): on the identical parse
  failure it calls `self.rt.log("warn", format!("stylesheet rejected ({}
  diagnostics)", diags.len()))`, and `crates/lumen-shell/src/lib.rs:404-417`
  additionally `eprintln!`s it.
- Not mentioned anywhere in `.claude/skills/styling-lss/SKILL.md`, which
  documents the hot-reload behavior accurately but doesn't distinguish the
  two load paths.

Consequence: a broken `include_str!("../app.lss")` at startup — the single
most common way an app first attaches a stylesheet — produces an app that
"works" but is entirely unstyled, with no signal anywhere in logs or
diagnostics pointing at the stylesheet as the cause.

Fix: route the initial-load path through the same `rt.log("warn", ...)` call
already used by `set_stylesheet`.

---

**7. `Action`s and their paired handlers are two independently-settable
fields with no structural link — a widget can declare an action it doesn't
implement, and the only check is an opt-in test helper.**

Evidence:
- `.claude/skills/writing-widgets/SKILL.md:140-146`: *"**Never declare an
  action you don't implement** (W2)… `Headless::audit_actions()` reports
  violations as `W0106` — assert it is empty in your widget's test."*
- Reproduced concretely: `crates/lumen-widgets/src/widgets.rs:248` declares
  `actions: vec![Action::SetValue, Action::Increment, Action::Decrement]` on
  the free-function `slider` with **no `on_key` handler at all** — no
  arrow/Home/End/PageUp/PageDown support — while the typed `Slider`
  (`slider.rs:114,139-155`) declares the same actions **and** implements
  them.
- `audit_actions()`/`W0106` is real and reachable, but nothing calls it
  automatically; it's a test-time opt-in, not a construction-time invariant
  or a default part of `App::lint()`.

Consequence: the semantic tree — the one thing the agent and AT read to
know what a control can do — lies about the free-function `slider`, with no
warning unless a test author remembers to assert `audit_actions().is_empty()`.

Fix: fold `audit_actions()`'s checks into `App::lint()`'s existing pass
(already wired to `ui.lint`, `crates/lumen-widgets/src/app.rs:1452` and
`crates/lumen-agent/src/lib.rs:416`), so it's on by default rather than an
opt-in test assertion; longer-term, replace the loose `actions: Vec<Action>`
+ five independent `Option<Handler>` fields with capability types whose
presence *is* the declaration (can't declare `Increment` without supplying
the closure).

### Medium

**8. `cx.scope` memoization is silently opt-in dependency tracking — forgetting
to read a dependency inside the scope produces stale cached output with no
diagnostic outside a test-time-only coherence oracle.**

Evidence: `crates/lumen-widgets/src/element.rs:696-706` (`BuildCx::scope`
doc: memoization is skipped/reused based on exactly what the closure read);
`.claude/skills/building-apps/SKILL.md:116-118`: *"every signal the subtree
depends on must be read inside the scope, or invalidation misses it"* — a
named, real bug class with only `assert_view_coherent()` (test-only) as a
safety net; not available as an ordinary `ui.lint` check despite the
coherence oracle (F0) already existing as infrastructure.

Consequence: an LLM deriving a value outside a `cx.scope` and only *using*
it inside produces a subtree that silently serves stale content on
subsequent writes to the signal it should have depended on — invisible
without a dedicated coherence test.

Fix: expose the coherence check as an `app.diagnostics`/`ui.lint`-reachable
warning (compare the scope's declared/cached read-set against the read-set
of a shadow re-run, at least in debug builds), not only as a manual test
assertion.

---

**9. Four confirmed instances of doc/skill drift specifically in the areas
this review covers, violating the project's own binding doc-currency rule.**

Evidence:
- (a) `docs/app-framework-readiness.md:192-208` (E2): *"Today there is **no
  `Widget` trait**"* — false; `LeafWidget`
  (`crates/lumen-widgets/src/element.rs:91-113`) shipped, with a real
  external consumer at `examples/chart`.
- (b) `.claude/skills/building-apps/SKILL.md:159-160`: *"Duplicate-id
  detection is not enforced yet (W0001 dead)"* — false; `audit::
  check_duplicate_ids` (`crates/lumen-widgets/src/audit.rs:102-127`) is
  implemented and reachable via `ui.lint`.
- (c) `.claude/skills/writing-widgets/SKILL.md:21-24` claims the 2026-07-20
  free-function-to-typed-struct migration is complete — true for 26 of 32
  wrapped functions, false for `button`/`checkbox`/`slider`/`scroll`/
  `text_field_basic`/`progress_bar` (Finding 1) — the highest-traffic widget
  in the codebase.
- (d) `docs/api-audit-1.0.md:20-22` (the "1.0 API freeze" audit itself):
  *"Constructors follow one convention: `fn widget(cx?, name, …) -> Element`"*
  — describes the pre-2026-07-20 free-function-only world; the current,
  binding convention is typed structs, and even setting that aside there
  are now 4 coexisting construction shapes (free function, self-stateful
  typed struct, builder-with-`.build(cx)`, and the undocumented
  public-field `LeafWidget` shape used by `LineChart`/`PieChart`).

Consequence: `AGENT.md`'s "Doc currency" rule exists specifically because a
2026-07 audit found ~30% drift and decided docs must update in the same
commit as behavior changes. This review — scoped to exactly the areas that
rule protects — found 4 more instances in roughly 90 minutes of adversarial
reading, in the specific documents (skills, the 1.0 audit) most likely to be
trusted by an AI consumer over reading every source file itself.

Fix: the four citations above are a ready punch list. Longer-term, pair
"migration complete" claims like `writing-widgets/SKILL.md:21-24` with a CI
grep (see Finding 1's fix / Top-10 item 9) so the claim is enforced, not
just written down.

---

**10. `Element`'s `scope_key`/`shared` fields carry an "always set together"
invariant enforced only by a doc comment, and panic deep in reconciliation
— not at the point of misuse — when violated.**

Evidence: `crates/lumen-widgets/src/element.rs:241` (`pub scope_key:
Option<IdHash>`) and `:247` (`pub shared: Option<Rc<Element>>`) are two
independent, doc-hidden-but-public fields whose pairing is documented only
in prose ("Set by `scope`; not authored"). Violating the pairing panics far
from the mistake: `crates/lumen-widgets/src/app.rs:2807`
(`.expect("validated by copy_span")`), `app.rs:2832`
(`.expect("shared stub carries its key")`), `app.rs:3136` — all inside tree
reconciliation, not at construction.

Consequence: low-likelihood (both fields are `#[doc(hidden)]` and normally
only set by `BuildCx::scope` itself) but real for any code hand-constructing
an `Element` via `..Element::default()` and touching these fields — the
panic's stack trace points at reconciliation internals, not the
`Element { .. }` literal that caused it.

Fix: bundle both into one private `Option<SharedStub { key: IdHash, element:
Rc<Element> }>`, making the illegal "one set, one not" state
unrepresentable.

---

**11. Widget re-export surface is split between a flat crate-root namespace
(typed structs) and submodule-qualified paths (free functions/theme), with
no rule predicting which a given name needs.**

Evidence: `crates/lumen-widgets/src/lib.rs:271-304` flattens `Button`,
`CheckBox`, `Slider`, etc. at the crate root, but `widgets`, `widgets_extra`,
`widgets_m1/m3/m4`, `theme`, `markdown`, `forms`, `boundary`, `motion`,
`asset` remain submodule-qualified — even though `Canvas`/`Image` (from the
very same `widgets.rs` file as the un-flattened `button`/`checkbox`) *are*
flattened (`lib.rs:300`: `pub use widgets::{Canvas, Image};`).

Consequence: `lumen_widgets::Button` works; `lumen_widgets::button` does
not (must be `lumen_widgets::widgets::button`) — for what a consumer
perceives as the same category of thing ("a widget-construction API"), with
no naming convention distinguishing the two.

Fix: either flatten the remaining free-function modules at the crate root
(matching `Canvas`/`Image`'s treatment) or, preferably, complete Finding 1's
fix so the free-function forms are pure shims and the flat/typed surface is
the only one that matters.

### Low

**12. Hook-naming inconsistency**: `Button::on_press`/`Card::on_press`
(`button.rs:77`, `card.rs:83`) vs. `widgets::button`'s parameter name
`on_click` and the underlying `Element::on_click` method
(`element.rs:467`) — the same concept named two ways depending which of the
two (both still live) construction paths a reader is in.

**13. `Color::srgb8` is a runtime fn while `Color::WHITE`/`new_linear` are
`const`**, for no stated domain reason
(`.claude/skills/writing-widgets/SKILL.md:255-257`); a small, real "why does
this one need a function call" surprise, undocumented anywhere but a skill's
gotcha list.

**14. The live-agent verification protocol splits "ready" into three
separately-sequenced calls** (implicit exists-wait, `ui.waitSettled`,
`ui.waitFor`) with a documented history of previously-unreadable
ambiguous-selector errors (`.claude/skills/verifying-apps/SKILL.md:142-161`)
— see Skills-as-API-smell §, entries 30-31, for detail. Adjacent to, not
inside, the app-authoring surface proper, so ranked Low for this review's
scope even though it is a real ergonomic gap.

---

## Silent-failure inventory

Every place a consumer mistake found in this review produces **no error and
no visible effect** — the highest-value section for a framework whose
primary consumer cannot see the screen.

1. **Known-but-unapplied `.lss` property** (39 of 89) — parses clean, zero
   diagnostic, zero visual effect. (Finding 2; `style.rs:502`.)
2. **`shadow: ... inset`** or a comma-separated shadow list — parses as a
   valid shadow declaration, produces no shadow. (`styling-lss/SKILL.md:50`.)
3. **`transition`/`animation` on a node with no stable `.id(...)`** —
   recognized, matched, values snap instead of animating; nothing signals
   why. (`styling-lss/SKILL.md:73-82`.)
4. **`@media container(...)` with no reachable `Element::container()`
   ancestor** — always evaluates false, indistinguishable from "the
   condition is genuinely false." (`styling-lss/SKILL.md:52`.)
5. **Initial `App::stylesheet(src)` parse error** — entire stylesheet
   silently dropped, zero log output. (Finding 6; `app.rs:223,344-348`.)
6. **Untracked signal read** (via a bare `Runtime` handle outside a tracking
   context, e.g. `effect`/`memo`/`cx.scope`) — produces a non-reactive,
   silently-stale read; confirmed by the framework's own test
   `untracked_read_does_not_subscribe` (`state.rs:1270-1286`).
7. **Writing to a signal nobody has subscribed to yet** — `set`/`update`
   finds zero subscribers and flushes nothing; a complete, silent no-op
   indistinguishable from success (`state.rs:966-970,1105-1114`).
8. **Scoped state addressed by a flat/rebuilt string from outside the
   build** — `rt.signal("row-3/v")` is a different, root-level signal from
   the `v` created inside `cx.scope(("row-3",))`, because identity *folds*,
   it doesn't concatenate. Documented in two skills as failing "silently —
   the write lands on a signal nothing reads"
   (`building-apps/SKILL.md:124-129`, `writing-widgets/SKILL.md:197-202`).
9. **Accidental signal-key reuse** across two logically distinct call sites
   — silently aliases two pieces of state into one shared slot; no error,
   by design (`signal_at` intentionally reuses on key match). (Finding 3.)
10. **Forgetting to read a dependency inside `cx.scope`** — the memoized
    subtree silently serves stale content on the next unrelated write;
    caught only by a test-time coherence oracle. (Finding 8.)
11. **`widgets_extra::menu(items)` free-function shim** — permanently inert
    (no path to `Menu::on_select`); looks fully configured, never fires a
    selection. (Finding 1.)
12. **Declaring an `Action` without its paired handler** (e.g.
    `widgets::slider`'s `Increment`/`Decrement`/`SetValue` with no
    `on_key`) — the semantic tree lies to the agent/AT; caught only by the
    opt-in `audit_actions()`/`W0106` check. (Finding 7.)
13. **A focusable element with no `.id(...)`** — can never actually receive
    keyboard focus despite being nominally tab-reachable
    (`writing-widgets/SKILL.md:151-154`); surfaced only by the `W0301` lint
    if run.
14. **`.id(...)` called on a node with `elide_semantics: true`** (e.g.
    anything built from `widgets::row`/`column` without clearing the flag)
    — the id, and the whole node, is spliced out of the semantic tree;
    selectors/focus/AT can never see it, and calling `.id()` on it compiles
    and returns normally (`writing-widgets/SKILL.md:155-158`).
15. **`.lss` class-selector typos** — a Rust-side `.class("btn")` and an
    `.lss`-side `.btn { }` rule are two independently-maintained string
    literals with no compile-time or runtime link; a typo compiles and
    silently applies no style. (Corroborates Finding 2's broader pattern.)
16. **`cx.register_command("tally.inc", ...)` / `MenuItem::new("tally.inc",
    ...)` id mismatch** — same string-coupling pattern; a typo in either
    means a menu item that looks fully configured but does nothing when
    invoked. (Code-in-anger walkthrough, item 4.)
17. **Cross-window state sharing via a re-used string signal key, with a
    typo in only one window** — the two windows silently diverge into
    separate state pools (no panic — it's now two distinct valid keys)
    rather than the intended shared state. (Finding 3.)
18. **A Unicode codepoint outside the bundled font's coverage** passed to a
    text element — compiles, "renders," shows as tofu (a missing-glyph box);
    no build-time signal; caught only post-hoc by the opt-in `ui.lint`/
    `W0402` check (`writing-widgets/SKILL.md:260-263`).
19. **An explicit `.style.height` on a text-bearing `Element`** — silently
    ignored by layout (a text node sizes to its glyphs); no diagnostic
    connects the ignored value to the "resize just adds empty space"
    symptom it causes (`writing-widgets/SKILL.md:245-248`).
20. **A misspelled keyword *value* on a property the runtime does apply** —
    e.g. `display: flext;` (typo for `flex`). `E0103`'s value-shape check
    (`crates/lumen-style/src/parser.rs:539-552`) accepts *any* string in
    keyword position (`parser.rs:545`: `(Value::Keyword(_), "keyword") =>
    true` — no allow-list of actual keywords), so this passes validation
    cleanly; `as_display` (`crates/lumen-style/src/style.rs:927-937`) then
    returns `None` for the unrecognized keyword and the property is simply
    never set. The parser's own comment concedes the lineage
    (`parser.rs:534-538`): "the code was defined-but-dead until now, and
    `apply()` silently ignored bad values." Distinct from entry 1 — this is
    a typo on a property that *is* implemented, not an unimplemented
    property.
21. **`transition:` declared on a layout property** (e.g. `transition:
    width 200ms;`) — a documented no-op
    (`crates/lumen-widgets/src/app.rs:975-976`): only paint-tier properties
    (background, opacity, etc.) actually animate; a transition rule
    targeting `width`/`height`/`padding`/anything layout-tier parses,
    matches, and produces no animation, with nothing distinguishing it from
    a working transition declaration.

---

## Skills-as-API-smell

Each `.claude/skills/` warning below is a candidate API defect: something
that ideally should be impossible or a compile error, not a paragraph an
agent has to remember. ~32 such warnings were catalogued across the six
skills; the most consequential are below, grouped by root cause, with a
fixability verdict. (Full 32-entry catalogue available on request; this
table is the load-bearing subset.)

| # | Warning (skill:location) | API gap it compensates for | Fixable? |
|---|---|---|---|
| 1 | *"Handlers capture only `Copy` state… `stable_handler!` makes violations fail to compile"* (`building-apps/SKILL.md:112-114`) | Widget constructors accept any `Fn + 'static`, no `Copy` bound — see Finding 5 | **Yes** — add the bound to every `on_*` parameter |
| 2 | *"every signal the subtree depends on must be read inside the scope, or invalidation misses it"* (`building-apps/SKILL.md:116-118`) | `cx.scope` has no static or lint-time dependency-completeness check — see Finding 8 | **Partial** — a dev-mode shadow-rerun diagnostic is feasible; full static solution is hard in general |
| 3 | *"identity *folds*, it does not concatenate… fails silently"* (`building-apps/SKILL.md:124-129`) | Root vs. scoped signal lookup share one ambiguous-looking API surface | **Yes** — a `ScopedKey`/`RootKey` newtype distinction, or a lookup-miss error instead of silent no-op |
| 4 | *"Duplicate-id detection is not enforced yet (W0001 dead)"* (`building-apps/SKILL.md:159-160`) | Stale — see Finding 9(b); the check is actually implemented | **Already fixed** — doc needs updating, not the API |
| 5 | *"A dotted id (`#faq.returns`) parses as id+class and is unselectable"* (`building-apps/SKILL.md:153-156`) | `.id(&str)` accepts any string; no charset validation at construction | **Yes** — a validating `WidgetId` newtype or a `debug_assert!` in `.id()` |
| 6 | *"Every widget is a typed struct. Never write a bare `pub fn … -> Element`"* (`writing-widgets/SKILL.md:14-15`) | No compiler/lint enforcement of the rule — see Finding 1 | **Yes** — CI grep for un-shimmed top-level `-> Element` fns (Top-10 item 9) |
| 7 | *"`elide_semantics`… the node *and its id* are spliced out… clear the flag or it is invisible"* (`writing-widgets/SKILL.md:155-158`) | Boolean field defaults to "invisible"; `.id()` silently no-ops on an elided node | **Yes** — invert the default, or make `.id()` a debug-assert/panic on an elided builder |
| 8 | *"Never declare an action you don't implement… `audit_actions()`… assert it is empty in your widget's test"* (`writing-widgets/SKILL.md:140-146`) | `actions: Vec<Action>` and `on_*: Option<Handler>` are structurally independent — see Finding 7 | **Yes** — capability enums that make declaration and implementation one field; short-term, fold the audit into `App::lint()`'s default pass |
| 9 | *"A focusable node needs a stable id… can never hold focus"* (`writing-widgets/SKILL.md:151-154`) | `focusable: bool` and `id: Option<String>` are independent fields | **Yes** — a `Focusable(StableId)` wrapper instead of a loose bool+optional-string pair |
| 10 | *"`Color::srgb8` is a runtime fn (not `const`)"* (`writing-widgets/SKILL.md:255-257`) | Pure API inconsistency | **Yes** — make it `const fn` |
| 11 | *"Leaf widgets have no intrinsic size and collapse to 0 when centred"* (`writing-widgets/SKILL.md:308-310`) | `widgets::leaf(...)` has no required-size parameter and no fallback | **Yes** — require a `Size` or default to a visible non-zero fallback with a lint |
| 12 | *"Without `--lib`, a bare filter matches integration-test files, runs zero unit tests, and still prints `ok` — a false green"* (repeated 3×: `writing-widgets/SKILL.md:368-370`, `verifying-apps`, `debugging-lumen`) | `cargo test` filtering ambiguity — arguably the single most dangerous silent failure catalogued (a green CI run that ran 0 tests) | **Yes, at the tooling layer** — a `just test <crate>` wrapper or a CI check for `0 passed` |
| 13 | *"`.lss` parses essentially the whole spec… the runtime applies a subset. Writing a parse-only property is a silent no-op"* (`styling-lss/SKILL.md:8-10`) | The central defect of this review — see Finding 2 | **Yes, and already on the team's own roadmap** ("Phase B") |
| 14 | *"`inset`/comma lists unsupported — `inset` disables the declaration"* (`styling-lss/SKILL.md:50`) | Partial syntax support degrades to total silent failure instead of a diagnostic | **Yes** — route to the existing `E0102`/`E0103` machinery |
| 15 | *"No id → snaps"* for transitions/animations (`styling-lss/SKILL.md:73-82`) | Runtime already knows at apply-time that the node lacks an id; no diagnostic fires | **Yes** — emit a lint when an animation/transition rule matches an id-less node |
| 16 | *"Ambiguous/NotFound errors are now readable and list `node-N` candidates"* (`verifying-apps/SKILL.md:157-158`) | Implies prior versions of the live-agent selector protocol produced unreadable errors — the `node-N` vs `#id` two-identity-space design is the structural cause | **Partial** — a distinct sigil for ephemeral vs. stable ids would remove the ambiguity at the grammar level |
| 17 | *"Live-window traps (each one has burned an agent)"* — auto-wait only covers existence; settling is `ui.waitSettled`; state is `ui.waitFor` (`verifying-apps/SKILL.md:142-153`) | Three separately-sequenced readiness primitives instead of one composed call | **Partial** — see Top-10 item 10 |
| 18 | *"`Runtime::resource(name, future)` (the old noop-waker form) is REMOVED"* (`building-apps/SKILL.md:171-172`) | Historical: the old form silently polled once and hung forever | **Already fixed, by deletion** — cited as the pattern other entries above should follow |

Two notable **non-smells** (the skill documents something the type system
*already* enforces, correctly, and the warning is belt-and-suspenders
rather than compensating for a real gap): `lumen-data-async/SKILL.md:73-74`'s
`Sink` re-entrancy warning is backed by `Runtime`'s genuine `!Send` shape
(`Rc<RefCell<...>>`, `state.rs:311-325`), so the dangerous case is already a
compile error, not just a documented discipline; and the contained-panic
resilience behavior in `debugging-lumen/SKILL.md:29` is a deliberate,
correct design choice (keep the last good frame), not a defect.

**Does `verifying-apps`' existence indicate the live-agent API has ergonomic
problems?** Yes, on the evidence of the skill's own framing. Its "Live-window
traps" section is headed *"each one has burned an agent"*
(`verifying-apps/SKILL.md:142`) — not hypothetical, but a catalogue of
incidents. Combined with entry 16's implicit admission that ambiguous-match
errors used to be unreadable, this is real evidence the introspection
protocol has accumulated footguns proportionate to its power, not just
documentation for an inherently-hard domain.

---

## Side-by-side vs iced/egui

### Counter — Lumen

`examples/iced-parity/src/counter.rs` (full file, 21 lines):
```rust
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::caption("COUNT"),
        theme::display(format!("{v}")).id("value"),
        theme::button_row(vec![
            theme::ghost_button("–", move |rt| count.update(rt, |c| *c -= 1)).id("dec"),
            theme::accent_button("+", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        ]),
    ])))
}
```

### Counter — iced (0.13-era canonical form; `examples/iced-parity` does not
vendor iced source or a README to diff against — this reconstruction is the
well-known, publicly documented iced counter shape, flagged as such rather
than a repo citation)

```rust
use iced::widget::{button, column, text};

pub fn main() -> iced::Result {
    iced::run("Counter", Counter::update, Counter::view)
}

#[derive(Default)]
struct Counter { value: i64 }

#[derive(Debug, Clone, Copy)]
enum Message { Increment, Decrement }

impl Counter {
    fn update(&mut self, message: Message) {
        match message {
            Message::Increment => self.value += 1,
            Message::Decrement => self.value -= 1,
        }
    }
    fn view(&self) -> Column<Message> {
        column![
            button("+").on_press(Message::Increment),
            text(self.value).size(50),
            button("-").on_press(Message::Decrement),
        ]
    }
}
```

**Honest assessment**: Lumen wins the structural comparison decisively.
There is no `Message` enum, no `match` in an `update` function, no `struct
Counter` to hold state — `cx.signal` *is* the state, colocated with the view
that reads it. iced's Elm-architecture ceremony (declare every possible
mutation as a variant, route it through a central `update`) is exactly the
boilerplate an LLM burns tokens getting right or wrong (missing match arms,
message-type mismatches between `view` and `update`). Lumen's closures
mutate state inline where the intent is clearest.

egui, for context, is even shorter for this one screen (immediate mode, no
signals at all — `if ui.button("+").clicked() { self.value += 1; }` inline
in a single `fn ui(&mut self, ui: &mut egui::Ui)`), but that brevity doesn't
generalize: egui has no retained semantic tree, so it has nothing resembling
Lumen's `ui.getTree`/stable-selector/coherence-oracle story, and every
interactive element's identity is positional/order-dependent, which is a
much worse foundation for an AI-driven test harness than Lumen's `#id`-keyed
approach — egui's simplicity is a trap for exactly the audience this
framework targets.

Dioxus (RSX macro, a JSX-alike over a virtual-DOM diff) and Slint (a
separate `.slint` DSL compiled to Rust, roughly analogous to what `.lss`
*could* be if it covered layout as well as paint) both require a second
language/macro DSL Lumen avoids for structure (Lumen's tree is plain Rust
function calls) — but Slint's DSL, being a first-class compiled artifact
rather than a runtime-parsed stylesheet with a silent-no-op subset, doesn't
have Lumen's specific Finding-2 problem: an unsupported Slint property is a
compile error, not a silent runtime no-op. That is the sharpest, most
literal point of comparison this review found: **the one place a
declarative-DSL competitor (Slint) does strictly better than Lumen is
exactly the place Finding 2 identifies as Lumen's worst gap** — turning
"recognized but unimplemented" into a compile-time error rather than a
silent parse-success.

Flutter's widget tree is structurally the closest analog to Lumen's
`Element` tree (composable, typed widgets, a semantics tree for
accessibility) but requires `StatefulWidget`/`State<T>`/`setState(() {...})`
ceremony Lumen's signals sidestep entirely — and Flutter has no equivalent
of Lumen's headless-deterministic-renderer-plus-agent-JSON-RPC combination,
which remains this review's strongest point in Lumen's favor across every
framework compared.

---

## Top 10 API changes

Ranked by (developer-pain relieved ÷ breakage cost).

**1. Require `Copy` on every handler closure parameter.** Breakage: near
zero (per ADR-013, a non-`Copy`-capturing handler is already a bug). Relief:
eliminates the most-cited recurring bug class across every skill.
```rust
// before
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element
// after
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + Copy + 'static) -> Element
```

**2. Make `widgets::button`/`checkbox`/`slider`/`scroll`/`text_field_basic`/
`progress_bar` real one-line shims over their typed structs**, matching the
other 26 already-correct shims. Breakage: medium (visual diff for anything
relying on the divergent look — which is the point). Relief: high — removes
Finding 1 entirely, including two shipped bugs.
```rust
// before
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + 'static) -> Element {
    Element::button(label).on_click(on_click)
}
// after
pub fn button(label: impl Into<String>, on_click: impl Fn(&Runtime) + Copy + 'static) -> Element {
    Button::new(label).on_press(on_click).into()
}
```

**3. Emit a diagnostic for known-but-unapplied `.lss` properties.** Breakage:
zero (pure addition). Relief: high, specifically for an AI that cannot
otherwise notice.
```rust
// before: apply()'s catch-all
_ => {}
// after
p if PARSE_ONLY_PROPERTIES.contains(&p) => diagnostics.push(
    Diagnostic::warn(codes::W0105, format!("`{p}` is recognized but not applied (parse-only)"), span)),
```

**4. Route initial-load `.lss` errors through the same `rt.log` the
hot-reload path already uses.** Breakage: zero. Relief: high for a whole
class of "why is my app unstyled" confusion.
```rust
// before
app_sheet: self.stylesheet.as_deref().and_then(parse_sheet),   // silent None on error
// after
app_sheet: self.stylesheet.as_deref().and_then(|s| parse_sheet_logged(s, &rt)),
// parse_sheet_logged mirrors set_stylesheet's rt.log("warn", ...) on failure
```

**5. Enrich the signal type-mismatch panic message.** Breakage: zero
(message-only). Relief: medium — turns a baffling panic into an actionable
one.
```rust
// before
.expect("signal type mismatch")
// after
.unwrap_or_else(|| panic!("signal {key:?}: stored as {stored_ty}, read as {}", std::any::type_name::<T>()))
```

**6. Fold `audit_actions()`/`W0106` into the default `App::lint()`/
`ui.lint` pass.** Breakage: low (an already-implemented check, now always-on).
Relief: medium-high — closes the "declared action, no handler" hole by
default.
```rust
// before
Headless::audit_actions()   // must be called explicitly in a test
// after
App::lint()                 // W0001/W0106-class checks folded into the default pass
```

**7. Re-export `theme` and `element::Shadow` from the facade** (and audit
the remaining un-re-exported `pub mod`s for similar gaps). Breakage: zero
(additive). Relief: medium — makes facade-only actually viable for the
patterns the example corpus exercises.
```rust
// crates/lumen/src/lib.rs — add
pub use lumen_widgets::theme;
pub use lumen_widgets::element::Shadow;
```

**8. Typed signal keys** (`SignalKey<T>`) declared once per logical piece of
state, replacing the inferred-`T`-per-call-site pattern. Breakage: medium
(touches the `cx.signal` call convention broadly). Relief: high, long-term —
removes both the type-mismatch-panic class and the silent-key-aliasing class
at the root.
```rust
// before
pub fn signal<T: State, K: Hash + Debug>(&self, key: K, init: impl FnOnce() -> T) -> Signal<T>
// after
const COUNT: SignalKey<i64> = SignalKey::new("count");
pub fn signal<T: State>(&self, key: SignalKey<T>, init: impl FnOnce() -> T) -> Signal<T>
```

**9. CI-gate the "no bare `pub fn -> Element` outside the primitives
allowlist" rule the skill already states as binding.** Breakage: zero (a new
CI check, not an API change). Relief: medium — prevents Finding 1/9(c)-class
drift from recurring.
```
# xtask / CI check
grep -rn '^pub fn .* -> Element' crates/lumen-widgets/src/*.rs \
  | grep -vE ':(27|43|50|118|123|128):'   # text/row/column/stack/leaf/keyed allowlist
# any other match fails the build
```

**10. A single composed `wait_until`-style live-agent RPC** replacing the
current three-call manual sequencing (implicit exists-wait, `ui.waitSettled`,
`ui.waitFor`). Breakage: medium (additive; existing granular calls can stay).
Relief: medium — closes the "agent forgets to also wait for settling" trap
`verifying-apps/SKILL.md` spends several paragraphs warning about.
```
# before
input.click(sel) → ui.waitSettled({timeout_ms}) → ui.waitFor({selector, state?, text?})
# after
ui.actAndVerify({action: {click: sel}, expect: {selector, state?, text?}, settle: true, timeout_ms})
```
