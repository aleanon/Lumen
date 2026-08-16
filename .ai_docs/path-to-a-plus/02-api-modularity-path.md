# Path to A+: consumer API and modularity

*Research note, 2026-08-07. Companion to `.ai_docs/review-2026-08/02-consumer-api.md`
(C+, B- on "easier for an AI than iced/egui"), `03-modularity.md` (B-), and
`/home/aleksander/.claude/plans/zippy-dancing-allen.md` (the approved campaign,
which predicts B-/B on these two axes and explicitly declines the four items
this note costs). Every repo claim below is cited `path:line`; nothing here is
re-derived from the campaign's own summary without checking the underlying
review.*

---

## Verdict

**Consumer API: A+ is reachable, conditionally.** The structural advantage
(no `Message` enum, `Copy`-signal reactivity, real agent introspection) is
already real and already the framework's actual differentiator — no
competitor benchmarked in this note (SwiftUI, Compose, Flutter, Dioxus,
Leptos, Xilem) has an equivalent. What stands between C+ and A+ is not a
research problem, it is the mechanical work the campaign already scoped and
then explicitly deferred (SD3, SD4, SD5, SD6a) plus two things the campaign
never scoped at all: **actually implementing** the mechanical/medium subset
of the 41 dead `.lss` properties (not 39 — §2 corrects the count) instead of
only diagnosing them, and a genuine type-system pass over the API-smell
catalogue (§3) instead of the diagnostics-only fix the campaign ships.
Conditional on: (1) shipping the type-level fixes in §3, not just the
runtime diagnostics the campaign adds; (2) implementing the ~24 mechanical
+ 9 medium properties (§2) before 1.0, not deferring them past the freeze;
(3) finishing the widget-completeness gaps §4 finds still open — smaller
than the Aug 3 widget review implies, since a source check for this note
found its two worst systemic blockers (disabled state, declared-vs-
implemented actions) already fixed as of 2026-08-03; what remains is the
arrow-key-navigation matrix's unfinished half, a `RadioGroup` container, and
four still-missing composable widgets, none of them structural.

**Modularity: A+ is reachable, but not without `register_property` and a
shared shell crate — both declined by the campaign, both costed below.**
The crate-boundary skeleton (cycle-free, `Renderer`/`LeafWidget` genuinely
open) is already A--adjacent. What caps it at B/B+ is architectural, not
mechanical: `Style::apply` is a closed `match`, so "third-party widgets and
styling are first-class" (`.ai_docs/01-architecture.md:11`) is true for
widgets and false for style properties — and that gap cannot be closed by
file reorganization the way `lumen-widgets`' split can.

**Neither reaches A+ on the campaign's current scope.** The campaign says so
itself: *"This campaign does not reach A+ anywhere. Getting further requires
work this plan explicitly declines: competitive benchmarking,
`register_property`, the shared shell crate, and implementing the ~39
missing `.lss` properties rather than just diagnosing them."*
(`zippy-dancing-allen.md:391-393`). This note takes that sentence as its
brief and prices every item in it — including correcting its own "~39,"
which this note's §2 traces to a line-count-vs-item-count error and
resolves to 41.

---

## 1. What A+ means for an AI-first API

Human DX benchmarks (readability, ceremony, IDE autocomplete) are the wrong
yardstick for a framework whose primary author cannot see the screen and
iterates by re-running a build, not by squinting at a preview pane. Five
properties define A+ for that consumer, in priority order:

1. **No silent no-ops.** Every mistake that compiles must either do what it
   looks like it does, or fail loudly — a diagnostic, a lint, a panic with
   the two disagreeing facts named. An AI cannot "notice" a control that
   looks right and does nothing; it has no eyes. This is the single
   dimension the current C+ score is dragged down on (21-entry silent-failure
   inventory, `02-consumer-api.md:657-749`), and it is the dimension none of
   SwiftUI/Compose/Flutter/Dioxus/Leptos share with Lumen's specific failure
   mode, because none of them have an untyped, closed-`match` styling layer
   sitting between a declarative surface and the render tree. Slint is the
   sharpest comparator here and it wins outright: an unsupported property in
   Slint's DSL is a **compile error**, never a silent parse-success
   (`02-consumer-api.md:882-894`).
2. **Exactly one way to do each thing, and the example corpus teaches it.**
   Two independently-maintained widget-construction paths (`widgets::button`
   vs `Button::new`, `02-consumer-api.md:215-251`) is a worse defect for an
   AI than for a human, because a human notices the visual drift; an AI
   trained on "whichever one appears more often in the corpus" (117:22) will
   reliably reproduce the wrong one. SwiftUI, Compose, and Flutter all
   converged on exactly one composition primitive per concern (a `View`, a
   `@Composable`, a `Widget`) specifically because ambiguity compounds in
   any corpus-trained generation, human or model.
3. **Wrong states unrepresentable in the type system, not enforced by
   convention.** `LeafWidget`/`NodeContent`'s enum already does this for
   node content (`02-consumer-api.md:88`); `Action`↔handler pairing,
   `scope_key`/`shared`, and signal-key↔type do not. Compose's
   `Modifier.Node` API is the sharpest industry precedent for "make the
   illegal state impossible instead of documenting it": a stateless
   `ModifierNodeElement` factory paired with a stateful `Node` is enforced by
   the trait shape, not a runtime check (see §5, `register_property`).
4. **Errors are actionable at the call site, not three layers into
   reconciliation.** `.expect("signal type mismatch")` with no key name
   (`02-consumer-api.md:319-355`) fails this; `stable_handler!`'s deliberately
   shaped compile error (`02-consumer-api.md:87`) is the model to generalize.
5. **The introspection surface is part of the API, not bolted onto it.**
   This is Lumen's actual point of difference and the one dimension where it
   is already ahead of every framework compared — `ui.getTree`/`ui.lint`/
   `assert_view_coherent` have no equivalent in SwiftUI, Compose, Flutter,
   Dioxus, or Leptos (`02-consumer-api.md:38-41, 896-902`). A+ means this
   surface covers every silent-failure class in §2 below, not just the ones
   convenient to instrument first.

None of the five require inventing new theory. #1 and #4 are what the
campaign's own SD5/K-series already do for a subset of cases; A+ means
finishing the set, not a different design. #3 is the one item that needs a
genuine (if small) type-system redesign — costed in §5.

---

## 2. The 39 properties, enumerated and triaged

**The number is wrong, and now resolved: it's 41, not 39, and not the
review's own alternate 52 either.** `KNOWN_PROPERTIES`
(`crates/lumen-style/src/properties.rs:4-89`) spans source *lines* 4-89, but
that range includes the `pub const` header, four category-comment lines, and
the closing `];` — it is **78** actual property-name entries, not 89. The
review mistook a line-number span for an item count. `APPLIED_PROPERTIES`
(`crates/lumen-style/src/style.rs:371-409`) is correctly 37, verified
independently and by the crate's own `applied_properties_change_a_style_and_only_they_do`
test (`crates/lumen-style/tests/style.rs:148-203`), which asserts — as a CI
gate, not just static analysis — that every `KNOWN_PROPERTIES` entry *not*
in `APPLIED_PROPERTIES` leaves `Style` unchanged. **78 − 37 = 41.** This
count must replace "39" everywhere the campaign's SD5.1
(`PARSE_ONLY_PROPERTIES` const + parity-test extension) references it, or
the enforced const will itself ship wrong.

### The one finding that changes the whole cost estimate

**26 of the 41 fail to apply for a single, structural reason: `apply()`
only ever touches `lumen_style::Style` (a paint-tier struct) — never
`lumen_layout::LayoutStyle` directly — and `LayoutStyle`/Taffy already
implement real behavior for every one of those 26.** `LayoutStyle`
(`crates/lumen-layout/src/style.rs:172-421`) already has fields for
`justify_content`/`align_items`/`align_self`/`align_content` (:190-196),
`flex_grow`/`flex_shrink`/`flex_basis` (:184-188), `min/max_width/height`
(:206-212), `aspect_ratio` (:214), `position`/`inset` (:178,220),
`row_gap`/`column_gap` (:198-200), and full CSS Grid
(`grid_template_columns/rows`, `grid_column/row`, :222-228) — and
`LayoutStyle::to_taffy()` (:267-352) already maps every one of them onto
Taffy 0.7 (`Cargo.toml:119`, which has native grid support). The only
missing piece is a **3-hop plumbing job repeated per property**: add the
field to `lumen_style::Style` → add a parse/match arm in `apply()` → add
the copy-across line in `apply_css_to_element()`
(`crates/lumen-widgets/src/app.rs:4346-4413`, the exact function that
already does this for `display`/`width`/`padding`/`gap` and a handful of
others). **This is not new engine work anywhere in the 26.** The text tier
shows a milder version of the same pattern: `font-family`,
`letter-spacing`, `selection-color`, `text-align`, and `text-overflow` all
have working machinery one layer down (parley fields, an unused ellipsis
truncator, a hardcoded-instead-of-parameterized selection color) that
`.lss` simply never reaches. Only four properties — `cursor`, `filter`,
`z-index`, and (partially) `transform` — have a genuine engine gap.

### Full triage

| Property | Category | Rust-side support today | Triage | Effort |
|---|---|---|---|---|
| justify-content, align-items, align-self, align-content | layout | `LayoutStyle` fields exist, wired to Taffy (`lumen-layout/src/style.rs:190-196,293-296`) | **Mechanical** | ~4h (shared keyword parser, 4 properties) |
| flex-grow, flex-shrink, flex-basis, flex-wrap | layout | `LayoutStyle` fields exist (:182-188,285-292) | **Mechanical** | ~2h |
| row-gap, column-gap | layout | `LayoutStyle` fields exist (:198-200); `gap` shorthand already applied (`app.rs:3359-3361`) | **Mechanical** | ~1.5h (needs longhand-vs-shorthand precedence) |
| min-width, min-height, max-width, max-height | layout | `LayoutStyle` fields exist (:206-212,306-310) | **Mechanical** | ~1.5h (exact copy of existing `width` handling) |
| aspect-ratio | layout | `LayoutStyle.aspect_ratio` exists (:214,313) | **Mechanical** | ~1-2h (small `w/h` grammar decision) |
| position | layout | `LayoutStyle.position` exists (:178,275-278) | **Mechanical** | ~30m |
| inset (+ 4 longhand sides) | layout | `LayoutStyle.inset: Edges` exists (:220,326-331) | **Mechanical** | ~1.5-2h (copy of `padding-*`/`margin-*` pattern) |
| overflow | layout/paint hook | Not a `LayoutStyle` field, but the `clip`/`StyleClip` machinery it needs already ships (`app.rs:3617-3720`) | **Mechanical** | ~1h (alias keywords onto `StyleClip`) |
| grid-template-columns, grid-template-rows | grid | `LayoutStyle` fields exist (:222-224,335-341); Taffy 0.7 has native grid | **Medium** | ~4-5h (track-list parser; `repeat()` needs expansion, `Unit::Fr` already tokenizes at `style.rs:995`) |
| grid-column, grid-row | grid | `LayoutStyle` fields exist (:226-228,342-349) | **Medium** | ~3h (`<line>[/<line>]` / `span N` grammar) |
| font-family | text | `TextStyle.family` exists, wired to parley (`lumen-text/src/lib.rs:160,197-200`) | **Mechanical** | ~1h |
| letter-spacing | text | `TextStyle.letter_spacing` exists, wired (`lib.rs:156,492`) | **Mechanical** | ~30m |
| selection-color | text | Parameterized paint fn exists but uncalled; a hardcoded `Color` is used instead (`app.rs:4046`) | **Mechanical** | ~1h |
| text-align | text | `TextAlign` enum threads through shaping but every call site hardcodes `Start` (`app.rs:3436,4049`) | **Mechanical/medium** | ~2-3h (must thread to *both* measure and paint or they desync) |
| text-overflow | text | An ellipsis truncator already exists, doc-commented, uncalled (`lib.rs:534-557`) | **Mechanical/medium** | ~2-3h |
| text-wrap | text | Wrap width already computed ad hoc from `style.width` (`app.rs:3423-3441`) | **Mechanical/medium** | ~1-2h |
| font-style | text | No `TextStyle` field; parley natively supports it, unexposed | **Medium** | ~3-4h (vertical slice: field + parley line + apply arm + cache-key update) |
| font-features, font-variation | text | Same — parley supports both natively, unexposed | **Medium** | ~4-6h each (font-variation's visible effect also needs a variable font registered) |
| text-decoration | text | Nothing exists (no underline/strikethrough anywhere in `lumen-text`) | **Medium** | ~3-4h (either parley's native decoration property, or an extra `DrawCmd::Rect`, mirroring the existing selection-rect pattern) |
| cursor | interaction | **Nothing exists** — no `Element.cursor` field, zero `CursorIcon` hits in the workspace | **Medium (hard-ish)** | ~1 day (new field + parse + winit `set_cursor` in the hover pipeline; well-scoped, no engine gap) |
| filter | paint | `BackdropFilter` exists for content *behind* a node; no foreground filter `DrawCmd` | **Medium (blur) / Hard (rest)** | ~2-4 days (`blur()` reuses the existing pipeline, `gpu.rs:59-61,747-769`; brightness/contrast/grayscale/hue-rotate/invert need new shaders) |
| transform, transform-origin | paint | `DrawCmd::PushLayer.transform: Affine` **already implemented** on both CPU and GPU backends, but every call site hardcodes `Affine::IDENTITY` (`app.rs:3695,3719`) — never exercised with a real value | **Medium (paint-only) / Hard (interactive parity)** | ~2-3 days — the paint path is nearly free; hit-testing (`Tree::hit_test`) still uses untransformed bounds, so a rotated/scaled node would be clickable in the wrong place unless hit-test is updated symmetrically |
| z-index | paint | **Nothing exists** — paint order is plain document-order recursion; no stacking-context sort, no matching hit-test-order change | **Hard** | ~2-3 days (genuinely new sibling paint-order sort, symmetric hit-test change, no reusable hook) |

**Roll-up**: 24 properties mechanical (≈1-2 engineer-days total), 9 medium
(≈2-3 engineer-days total), 4 genuinely hard (`cursor`, `filter`, `transform`
+ hit-test, `z-index` — ≈1.5-2 engineer-weeks total, mostly `z-index` and
`filter`'s non-blur functions). **Full 41-property closure is a 2-3 week
job for one engineer, not a research problem** — the review's framing of
this as "missing properties" undersold how much of the underlying capability
already exists and is simply unwired. This directly contradicts the
campaign's framing that implementing the properties is out of scope for a
"quick win" pass; the mechanical 24 alone (1-2 days) is cheaper than the
diagnostic-only fix (SD5.1) makes it sound, and should ship *instead of*,
not merely alongside, a "parse-only, here's a warning" diagnostic for that
subset. The diagnostic remains the right answer only for the 4 hard cases
and as a permanent safety net against the *next* unwired property (the
`style_parity!`-extension part of SD5.1 is correct and should ship
regardless).

---

## 3. The skills-as-API-smell catalogue — type-level fixes

The review published 18 of ~32 catalogued entries (`02-consumer-api.md:761-780`;
the remaining 14 were "available on request" and not obtained for this note
— the ratio below is extrapolated from the published subset and should be
treated as directional, not exact). Of the 18 shown: **12 rated "Yes"
fixable, 3 "Partial," 2 already fixed (stale skill text), 1 fixable at the
tooling layer.** If that ratio holds over the full 32, roughly **20-22 are
closeable in the type system**, not just documentable. Below are concrete
signatures for the ten highest-leverage ones — "concrete" meaning each
change makes the mistake either not type-check or not compile, not "emits a
better warning."

**1. `Copy` bound on every handler parameter** (already specified as Top-10
item 1 in the review, `02-consumer-api.md:910-918`) — apply it everywhere,
not just `widgets::button`:
```rust
// before (every on_* setter in the typed-struct family and every free function)
pub fn on_press(mut self, f: impl Fn(&Runtime) + 'static) -> Self
// after
pub fn on_press(mut self, f: impl Fn(&Runtime) + Copy + 'static) -> Self
```

**2. `SignalKey<T>` for flat state** — closes both the type-mismatch panic
and the silent-key-aliasing failure at compile time. Full design in §6.

**3. `Focusable`, not `focusable: bool` + `Option<String>` independently.**
The Aug 3 widget review's own "lessons learned" names this exact trap:
"focus is keyed by `StableId`, so a focusable node without an id can never
hold focus" (`docs/review-widgets-2026-08.md:261-266`).
```rust
// before
pub focusable: bool,
pub id: Option<StableId>,
// a widget can set focusable: true with id: None and compile cleanly — Tab silently skips it

// after
pub focusable: Option<Focusable>,           // None = not focusable
pub struct Focusable(pub StableId);         // cannot exist without an id
impl Element {
    pub fn focusable(mut self, id: StableId) -> Self {
        self.focusable = Some(Focusable(id)); self
    }
    // the old `.focusable(bool)` setter that took no id no longer exists
}
```

**4. `WidgetId` validating newtype for `.id(&str)`.** A dotted id
(`#faq.returns`) parses as id+class and is silently unselectable
(`02-consumer-api.md:767`). Validate at the one construction point instead
of documenting the charset:
```rust
pub struct WidgetId(Box<str>);
impl WidgetId {
    pub fn new(s: impl Into<Box<str>>) -> Result<Self, InvalidIdError> {
        let s = s.into();
        if s.contains(['.', '#', ' ']) { return Err(InvalidIdError::new(&s)); }
        Ok(Self(s))
    }
}
// .id() takes impl TryInto<WidgetId>, or a debug_assert! at minimum if a hard Result
// is judged too breaking pre-1.0 (it isn't — this is exactly the kind of breakage
// the freeze should absorb before 1.0, not after).
```

**5. Elided nodes cannot accept `.id()` at all — a marker-typed `Element`,
not a boolean flag.** This is the one fix in this table that ripples widest,
and is presented with that cost stated plainly. Today `elide_semantics`
splices a node and its id out of the semantic tree, and calling `.id()` on
an elided builder compiles and silently no-ops
(`02-consumer-api.md:705-709`, `docs/review-widgets-2026-08.md:268-272`
independently confirms the same trap bit the widget-fix pass itself while
building `PickList`'s trigger). Making the illegal call not exist:
```rust
pub struct Element<S = Semantic> { .. , _marker: PhantomData<S> }
pub struct Semantic;
pub struct Elided;

impl Element<Semantic> {
    pub fn id(mut self, id: WidgetId) -> Self { .. }   // only exists here
}
impl<S> Element<S> {
    pub fn clear_elide(self) -> Element<Semantic> { .. }  // the escape hatch, explicit
}
// row()/column() return Element<Elided> by default; .id() on the result is now
// E0599 (method not found), not a silent no-op.
```
**Cost**: every function returning `Element` today would need to specialize
its return type or stay generic over `S`, which is real API-wide churn.
Given it's pre-1.0 and this specific trap has already bitten the framework's
own contributors mid-fix, it belongs in the pre-freeze list (§9), not
deferred — but it is the one item on this list that should get a dedicated
design spike before committing, not just implemented from this sketch.

**6. Capability types replace `actions: Vec<Action>` + five independent
`Option<Handler>` fields.** This is Finding 7's own proposed direction
(`02-consumer-api.md:517-520`), specified:
```rust
pub enum Interactive {
    None,
    Clickable    { on_click: Handler },
    Adjustable   { on_increment: Handler, on_decrement: Handler, on_set_value: Handler<f64> },
    Dismissible  { on_dismiss: Handler },
    Selectable   { on_select: Handler<usize> },
}
// Element carries `interactive: Interactive`, not `actions: Vec<Action>` + loose Option fields.
// build_semantics derives the emitted `actions` list FROM `interactive`'s variant —
// declaring Increment without an on_increment closure is now a value that cannot be
// constructed, not a runtime audit that might not run.
```
This also **subsumes W0106**'s entire job (`audit_actions()`, currently
opt-in) — a declared-action-without-handler stops being a thing to *detect*
because it stops being a thing to *construct*. Fold `audit_actions()` into
`App::lint()` regardless (SD4, already scoped) as the transition safety net
while widgets migrate to `Interactive`.

**7. `SharedStub`, not two independently-optional fields.** Already
specified in the review (`02-consumer-api.md:606-608`) — `scope_key: Option<IdHash>`
and `shared: Option<Rc<Element>>` bundle into one
`Option<SharedStub { key: IdHash, element: Rc<Element> }>`, making
"one set, one not" unrepresentable instead of a doc-comment-only invariant
that panics deep in reconciliation when violated.

**8. `Color::srgb8` as `const fn`.** Pure inconsistency, zero design
cost (`02-consumer-api.md:772`).

**9. `leaf()` requires a size or the type carries a documented fallback.**
Leaf widgets with no intrinsic size collapse to zero when centered
(`02-consumer-api.md:773`):
```rust
// before
pub fn leaf(content: impl LeafWidget + 'static) -> Element
// after
pub fn leaf(content: impl LeafWidget + 'static, min_size: Size) -> Element
// or, if call-site ergonomics matter more than forcing the decision:
// keep the optional form but change LeafWidget::measure's default to a visible
// non-zero size (e.g. 24x24) instead of Size::ZERO, and add a W-code lint for
// any leaf that resolves to zero at layout time
```

**10. Root vs. scoped signal addressing become different types, closing
the flat-string-escapes-a-scope hole.** `rt.signal("row-3/v")` from outside
a `cx.scope(("row-3",), ..)` is a different, root-level signal from the `v`
created inside it, because identity *folds*, it does not *concatenate*
(`02-consumer-api.md:682-687`). The type-level fix: **`Runtime`'s bare
`signal`/`scope`/`memo`/`effect` methods stay root-only by construction** —
a `ScopeCx<'_>` (only obtainable from inside a running scope) is the *only*
type with a `signal` method that folds into the enclosing scope's hash.
There is then no way to spell a scoped key from outside the scope that
created it, because the method that would let you doesn't exist on `Runtime`
directly:
```rust
impl Runtime {
    pub fn signal<T: State>(&self, key: SignalKey<T>, init: impl FnOnce() -> T) -> Signal<T>;
    // no generic Hash+Debug key here anymore for ad hoc string addressing
}
impl<'a> ScopeCx<'a> {
    pub fn signal<T: State, K: Hash + Debug>(&self, key: K, init: impl FnOnce() -> T) -> Signal<T>;
    // this is the only place a raw string/enum key can fold into scope identity
}
```
This is a bigger migration than items 1-9 (every `cx.scope(key, |cx| ..)`
call site's closure parameter changes from `&BuildCx`/`&Runtime` to
`&ScopeCx`), which is why it's ranked last, but it is the one fix that
removes the failure mode structurally rather than diagnosing it — the
K-series' `W0003` (§6) is the correct, cheap interim step; this is what
closes the hole for good.

**Not fixable in the type system, listed for completeness**: `cx.scope`
forgetting to read a dependency (item 2 — a static full-coverage check is a
research problem; the dev-mode shadow-rerun diagnostic the review proposes
is the right *practical* ceiling); the live-agent three-call readiness
sequence (item 17 — an ergonomics/API-surface fix, not a type-safety one,
already Top-10 item 10 in the review); anything already fixed by a stale
doc correction (items 4, 18) needs a docs commit, not code.

---

## 4. Widget completeness vs Flutter/SwiftUI

**Not structural — the catalogue is broad and the systemic blockers are
mostly already fixed.** `docs/review-widgets-2026-08.md` (2026-08-03) found
four cross-cutting gaps and marked its own W1-W5 remediation plan
"IMPLEMENTED" the same day. A source check for this note confirms the two
worst ones actually landed:

- **S1 (nothing can be disabled)** — fixed. `Element::disabled: bool`
  (`crates/lumen-widgets/src/element.rs:168`) plus `.disabled(bool)` in the
  common builder (`crates/lumen-widgets/src/widget.rs:32`) reach every typed
  widget at once.
- **S2 (declared actions with no handler)** — fixed for `Slider`.
  `on_increment`/`on_decrement`/`on_set_value` are wired
  (`crates/lumen-widgets/src/slider.rs:127-133,174-177`). `02-consumer-api.md`
  Finding 7 independently confirms the typed `Slider` "declares the same
  actions **and** implements them" — the gap it found is specific to the
  untyped free-function duplicate (Finding 1 / SD3), not the typed widget
  itself.
- **S3 (no arrow-key interaction)** — partially fixed. `on_key` is now
  implemented in 9 files (`text_input`, `text_field`, `widgets_m1`,
  `widgets_m4`, `pick_list`, `range_slider`, `scrollable`, `slider`,
  `app.rs`'s dispatcher), up from the 3 the Aug 3 review found. **Still
  missing**, confirmed by grep against current source: `Combobox`, `Tabs`,
  `Menu` (`widgets_extra.rs`), `Grid`/`Tree`/`DataGrid` 2-D navigation. Radio
  group arrow-nav is blocked on a real design gap the review already named
  correctly: an individual `Radio` cannot see its siblings, and registering
  group membership during `build` would make `build` impure — this needs a
  `RadioGroup` container widget, not a patch to `Radio`.
- **New widgets since the Aug 3 review**: `Card` and `Badge` now exist
  (`crates/lumen-widgets/src/card.rs:122` for `Badge`). Still absent, per a
  fresh grep: `SegmentedControl`, `ListTile`, `Rating`, `Breadcrumb`.

**What this means for A+:** the widget catalogue's *breadth* is already
close to A+ against Flutter's Material set (per `.ai_docs/09-flutter-widget-reference.md`,
the ~58 typed widgets cover the load-bearing 80%: `Grid`/`DataGrid`/`Tree`/
`VirtualList` are described by the review as "in good shape" and exceed what
Dioxus or Leptos ship out of the box — neither has a first-party virtualized
data grid). The remaining work is **finishing, not inventing**: complete the
arrow-key matrix (mechanical, WAI-ARIA patterns are well documented, ~1-2
days per widget family), build `RadioGroup` (small, new container widget),
add the four missing widgets (`SegmentedControl`/`ListTile`/`Rating`/
`Breadcrumb` are all composition over existing primitives per the review's
own note — "not blockers, each is composable today"), and land SD3 (the
free-function-to-typed-struct shim fix, already scoped in the campaign's
M-A) so the fixes above reach both widget-construction paths, not just the
typed one. None of this is a widget-completeness gap that blocks A+ on its
own; it is the same mechanical-fix pattern as the silent-failure inventory.
The one item that *is* a genuine design gap (S4's per-instance id
namespacing, and the id-less-focusable-node trap the Aug 3 review's
"lessons learned" section documents) is folded into the type-level fixes in
§3 below (a `Focusable(StableId)` wrapper).

---

## 5. Extension-point design: `register_property` and a shared shell crate

### `register_property` — costed against source, not against speculation

`Style` (`crates/lumen-style/src/style.rs:119-195`) is a flat, non-generic
struct of ~30 `Option<T>` fields with **no side-table** for extension data.
`apply()` (`style.rs:413-504`) is a single closed `match property { "display"
=> ..., _ => {} }`, cross-checked against a parallel `APPLIED_PROPERTIES`
const by the `style_parity!` test (`style.rs:366-370`). `Value`
(`crates/lumen-style/src/ast.rs:148-163`) — the parsed literal a property's
declaration carries — is itself a closed enum (`Number`, `Color`, `Keyword`,
`Str`, `Var`, `Function`, `List`); a third party cannot introduce a new
literal *type* without forking `ast.rs`, only reuse the existing variants
under a new property name.

**The performance objection in §8 is softer than it first looks.** `apply()`
has exactly three call sites, none of them "every property, every frame":
`restyle_subtree` (state-driven re-resolve, only nodes whose selector-relevant
state changed, `app.rs:2322`), a one-time keyframes precompute on stylesheet
load (`app.rs:2553`), and `build_node`'s style resolution — but that third
site is gated by a memo cache keyed on `(id, classes, states, ty,
overlay-hash)` (`app.rs:3318-3357`): `apply()` only runs on a **memo miss**,
i.e. once per genuinely new style key, not per node per frame. The real
constraint on a registration hook is "cheap per unique style key," which is
a far weaker bar than "cheap per frame."

**A precedent for the registry mechanism already ships.** `#[state_registry]`
(`crates/lumen-macros/src/lib.rs:389-408`) generates exactly the shape
`register_property` needs: a `static R: OnceLock<DynRegistry<dyn Trait>>`
backed table (`lumen-core/src/registry.rs:29`) with an `insert(name, ...)`
call at startup, used today for snapshot-restore of trait objects by name.
`app.rs:4520` independently confirms `OnceLock` statics are an accepted
idiom in `lumen-widgets` itself, not just macro-generated code. This is not
a new pattern for the codebase to adopt — it is the existing one, reused.

**Design (phase 1 — ships without touching `Value`, covers the realistic
majority of third-party properties):**

```rust
// crates/lumen-style/src/registry.rs (new)
pub type ApplyFn = fn(&mut Style, &Value, &Tokens) -> Result<(), PropertyError>;

pub struct PropertyDescriptor {
    pub name: &'static str,
    pub shape: ValueShape,   // reuses E0103's existing shape-check enum
    pub apply: ApplyFn,
}

/// Panics at registration time (startup, not per-declaration) if `name`
/// collides with a built-in KNOWN_PROPERTIES entry — cheap, and it turns a
/// third-party naming collision into an immediate, attributable failure
/// instead of a silently-shadowed built-in.
pub fn register_property(desc: PropertyDescriptor);
```

`Style` gains one field, allocated lazily so the common (no third-party
properties registered) case pays nothing extra in the struct's size:

```rust
pub struct Style {
    // ...existing ~30 Option<T> fields, unchanged, zero migration cost...
    custom: Option<Box<HashMap<&'static str, CustomValue>>>,
}
```

`apply()`'s existing `_ => {}` fallthrough becomes the one edited line:

```rust
_ => match registry::lookup(property) {
    Some(desc) => if let Err(e) = (desc.apply)(style, value, tokens) {
        diagnostics.push(Diagnostic::error(codes::E0104, e.to_string(), decl.span));
    },
    None if PARSE_ONLY_PROPERTIES.contains(&property) => diagnostics.push(
        Diagnostic::warn(codes::W0107, format!("`{property}` is recognized but not applied (parse-only)"), decl.span)),
    None => {}   // genuinely unknown — E0102 already caught this at parse time
},
```

The parser (`crates/lumen-style/src/parser.rs`) also needs registered names
folded into its known-property check so `E0102`'s did-you-mean and the
accept path recognize them — mechanical, since the registry is populated
before the first parse. `ui.getStyles` should expose `CustomValue` entries
alongside built-in ones (via a `Debug`/`to_json` fn on the descriptor) so a
third-party property is as introspectable to an agent as a built-in one —
this is what resolves tension #3 in §8.

**Phase 2 (optional, genuinely harder, not required for A+ on its own):**
opening `Value` itself (`Value::Custom(Box<dyn Any + Send + Sync>)` fed by a
per-property registered parser, not just an apply fn) — needed only for a
third party wanting a wholly new literal syntax (a custom color space, a
bespoke unit). Most realistic third-party properties (a new named preset, a
custom easing curve as `Function("cubic-bezier", [...])`, a boolean flag as
`Keyword`) are expressible in the existing closed `Value` today. **Recommend
shipping phase 1 only and revisiting phase 2 if a real third-party consumer
hits the wall** — speculative generality here is exactly the kind of
work a pre-1.0, no-external-consumers project should defer per its own
precedent (ADR-021's design log explicitly used this reasoning to skip
work with no current consumer).

**Effort**: phase 1 (registry + `Style` side-table + `apply()` edit + parser
recognition + `style_parity!` collision check + `ui.getStyles` exposure +
tests) is **3-5 days** for one engineer — small relative to its payoff,
because the hard infrastructure (`OnceLock`-backed registry, memo-gated
`apply()` call site) already exists in the codebase for a different purpose.

### Shared shell crate — costed against source, not against speculation

Confirmed by a byte-diff, not estimation: `render_into`
(`crates/lumen-shell-ios/src/lib.rs:30-52`, `crates/lumen-shell-web/src/lib.rs:22-44`)
is **identical**, 23 lines, in both crates. Session boot-or-resize, the
screenshot-to-buffer copy, and the pointer/text event-translation trio are
near-identical in shape across iOS and web (differing only in which
`RefCell`-shaped container holds the session), while Android's equivalents
are **not** directly shareable — its buffer write is a stride-aware,
safe-area-offset native-window blit (`imp.rs:339-361`), not a copy into a
caller-owned contiguous slice.

```rust
// crates/lumen-shell-core/src/lib.rs (new, thin, no FFI)
pub fn render_once(build: impl Fn(&mut BuildCx) -> Element + 'static,
                    size: Size, lss: Option<&str>, out: &mut [u8]) -> usize;

pub struct Session { /* Headless + last-known size */ }
impl Session {
    pub fn ensure(&mut self, build: impl Fn(&mut BuildCx) -> Element + 'static,
                  size: Size, lss: Option<&str>);
    pub fn copy_frame(&self, out: &mut [u8]) -> usize;
    pub fn inject_pointer(&mut self, phase: PointerPhase, pos: Point);
    pub fn inject_text(&mut self, text: &str);
}
```

**Estimated movement**: ~35-45 lines removed from `lumen-shell-ios` (136 →
~95), ~35-45 removed from `lumen-shell-web` (230 → ~190), ~10-15 removed
from `lumen-shell-android`'s boot/resize block only (its blit path stays
platform-specific). Net: a ~60-90 line `lumen-shell-core` removes ~80-110
duplicated lines combined — a modest mechanical win whose real value, as
the modularity review already noted, is **preventing future drift**
(iOS is already missing key/wheel/agent-bridge support web has, with
nothing in the build enforcing parity) rather than fixing something broken
today. **Effort: 3-5 days**, including headless conformance tests run
against all three shells — Android is emulator-verifiable in this dev
environment; iOS remains headless-only per this project's own constraints.

**Is A+ modularity possible without either of these?** No, on the
architecture doc's own terms. `.ai_docs/01-architecture.md:11` claims
*"third-party (and agent-written) widgets are first-class"* (true, via
`LeafWidget`) and by omission implies parity for styling, which is false —
`register_property` is what makes that claim actually true across the
board. The shared shell crate is not required for the *claim* to be
true (no doc asserts platform-shell parity as an extension point), but it
is required for the **B- → A/A+ jump specifically on Finding 6** (already-
diverged feature parity between platforms with nothing structural preventing
further drift) — without it, a fourth platform shell (or the next feature
added to one of the three) has no contract forcing it to match the others,
which is a modularity defect independent of whether register_property ships.

---

## 6. State/reactivity: the A+ design

### What the project already tried and correctly rejected

`docs/plan-reactive-derive.md` (RD-series, Xilem-shaped `#[derive(Reactive)]`
over one owned state struct — the SwiftUI-`@State`/Xilem authoring model)
was **retired the day it was written**, before any phase started, after
three independent reviews. The reasons are structural, not stylistic, and
apply regardless of who re-proposes the shape:

1. **Derive state can only ever be root-owned.** `BuildCx::signal` folds
   into the enclosing `cx.scope`'s prefix hash; a `Read<T>`/`Write<T>`
   holding only a `&Runtime` has no ambient scope to fold into, so it cannot
   address scope-local (per-row-in-a-list) state at all
   (`plan-reactive-derive.md:24-32`).
2. **It doesn't survive contact with the widget set.** Widget-owned state
   is string-keyed *public API* — `{name}.open`, `{name}.selected`,
   `{name}.page` across at least 13 widget files — and a derive app that
   opens a `Sheet` still writes `cx.signal("cart.open", …)`. The string-key
   surface the derive exists to eliminate is not eliminable from the app
   layer, so every non-trivial derive app becomes a hybrid of two authoring
   styles (`plan-reactive-derive.md:33-40`).
3. **It leaks state on list mutation.** `evict_scope` reclaims by walking
   `Slot.owner` through the scope tree; root-owned derive slots are never
   swept, so `todos[47].*` survives `todos.remove(47)` forever unless
   `lumen-core`'s eviction semantics change too — contradicting the plan's
   own "nothing in `lumen-core` changes" premise
   (`plan-reactive-derive.md:42-48`).

**This note does not re-propose that shape.** SwiftUI's `@State`/Compose's
`remember` avoid this exact problem by binding identity to *source location
+ call order* inside a persistent tree that survives across frames — Lumen
has no persistent `Element` tree (`Element` is consumed every `build_node`
pass, `05-architecture.md:139-141`), so the SwiftUI/Compose answer is not
transplantable without the retained-graph work the CP-series gates on CP5,
which is explicitly still an open architectural question, not a styling
change. Re-deriving identity from source position is a different, larger
bet than anything scoped in this note.

### The design that ships: typed keys for flat state, K-series for the rest

The project's own successor plan, `docs/plan-state-keys.md` (K-series),
attacks the *actual* measured failure directly instead of replacing the
authoring model, and is the right foundation — but it stops short of A+
because K1 is a **runtime diagnostic** (`W0003`, warn-not-panic), not a
compile-time guarantee. A+ needs one more piece on top of it:

**Layer 1 — `SignalKey<T>`, for flat/top-level named state (new, not in the
campaign).** This is Top-10 item 8 from the consumer API review
(`02-consumer-api.md:989-1000`), specified fully:

```rust
pub struct SignalKey<T> {
    name: &'static str,
    _marker: PhantomData<fn() -> T>,   // no drop-check bound, no Send/Sync leak
}

impl<T: State> SignalKey<T> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, _marker: PhantomData }
    }
}

// declared once, at module scope — not invented per call site
const COUNT: SignalKey<i64> = SignalKey::new("count");

impl BuildCx {
    pub fn signal<T: State>(&self, key: SignalKey<T>, init: impl FnOnce() -> T) -> Signal<T> { .. }
}
```

Every call site that means "the same piece of state" now references the
same `const`, so: a **type mismatch across two call sites becomes a
compile error** (both must specify `SignalKey<i64>` to compile against the
same const — there is no way to write `SignalKey<i64>` at one site and
`SignalKey<String>` at another for the same name without two different
consts, which is now visibly two different keys); a **typo becomes an
unresolved-identifier compile error** instead of a new, silently valid
string. This closes Finding 3's two failure modes (opaque panic,
silent-aliasing) at the type level rather than diagnosing them at runtime —
exactly the "wrong states unrepresentable" bar this note sets in §1.

**Layer 2 — typed enum keys for scoped/list state (already exists, needs
only to become the taught default).** `cx.scope(Field::Row(id), …)` and
`rt.signal(Field::Row(id), …)` already work today (ADR-021, `impl Hash +
Debug`), are already allocation-free, and Rust already rejects duplicate
enum variants at compile time (`plan-state-keys.md:129-136`). The gap is
purely pedagogical: the example corpus and `building-apps` still show ad
hoc `format!("row-{i}")` strings in places an enum key would be both safer
and faster. This is a docs/example fix, not new machinery — fold it into
SD2/the example-corpus cleanup already scoped for facade dogfooding.

**Layer 3 — K1's typed-collision diagnostic, for the one legitimate
escape hatch.** Widget-internal cross-widget contract state
(`{name}.open` etc.) is a real, intentional exception to "exactly one way"
— it must stay string-keyed because it is public API a third party's app
code addresses without importing a const the widget author controls. K1
(`W0003`, warn on same-key-different-type, silent on same-key-same-type)
is the correct, already-scoped safety net for exactly this case
(`plan-state-keys.md:95-121`) and should ship regardless of Layers 1-2. K2
(the optional `#[derive(StateKey)]` for an enumerable key space, giving
`ui.whatDependsOn` visibility into declared-but-unread keys) is worth
building only if K2.2/K2.3's agent-vocabulary benefit justifies it on its
own — the plan says so itself and this note agrees (`plan-state-keys.md:141-142`).

**Net:** three layers, one authoring style for app state (typed keys, either
`SignalKey<T>` or an enum), one clearly-scoped exception (widget-contract
strings) with a real safety net (K1), and no second reactivity model. This
is a smaller, cheaper design than RD-series and delivers what RD-series
actually promised (no more silent key aliasing) without the three faults
that got it retired.

### What Leptos/Dioxus/Xilem settled on, and why it doesn't transplant directly

- **Leptos**: `RwSignal<T>`/`ReadSignal<T>`/`WriteSignal<T>` are themselves
  the "key" — a signal is created once (`create_rw_signal`) and the *handle*
  (a `Copy` struct wrapping a slot index) is threaded through closures and
  props, never re-addressed by a string on every render pass. This works
  because Leptos's component tree is genuinely retained (fine-grained DOM
  node ownership), so a signal's owner-scope disposal is structural, not a
  re-derived key. Lumen's `cx.signal(key, init)` re-addressing-by-string
  exists specifically because Lumen's `build()` re-runs and re-attaches to
  persistent store state on every pass, with no retained view tree to hang
  a `Copy` handle off of long-term — this is the CP5/retained-graph question
  again, not a reactivity-API question. `SignalKey<T>` is the closest analog
  reachable without that larger architectural bet.
- **Compose**: `remember { mutableStateOf(...) }` binds identity to
  *call-site position in the composition slot table*, with an explicit
  `key()` composable for the cases (list items) where position isn't a
  reliable identity — structurally the same shape as Lumen's
  `cx.scope(explicit_key, …)`, just without Lumen's flat-string default.
- **Xilem**: view-tree diffing gives free structural identity for anything
  that doesn't need cross-rebuild persistent mutable state; state that does
  need it is owned in the app's data model and threaded by `&mut` — the
  model RD-series tried to port, rejected above for Lumen-specific reasons.

None of the three have Lumen's specific string-key-typo-aliasing failure
mode, but only because none of them address state by a bare string as the
*default* path — they default to a handle or a structural position, with an
explicit key only for the list-diffing edge case. `SignalKey<T>` +
typed-enum-for-scoped-state is Lumen's way of reaching the same property
(no bare, unchecked string as the default) without adopting a retained view
tree it doesn't have yet.

---

## 7. Crate structure at A+

### What's actually inside the 26k-LOC (17,070 non-test) crate

Source-grounded breakdown of `crates/lumen-widgets/src/` (51 files, flat, no
subdirectories):

| Subsystem | LOC | Files | Contents |
|---|---|---|---|
| Widget catalogue | 9,100 | 34 | `button.rs`, `card.rs`, `slider.rs`, `grid.rs`, `widgets.rs`/`widgets_extra.rs`/`widgets_m1/m3/m4.rs` (the milestone-named files SD2 already plans to regroup), etc. |
| App runtime | 4,613 | 1 | `app.rs` alone — `App`, `Headless`, `AppSnapshot`/`Checkpoint`, the `AnimVal`/`PropAnim` property-animation engine |
| App-toolkit | 948 | 6 | `forms.rs`, `nav.rs`, `undo.rs`, `i18n.rs`, `system.rs`, `tasks.rs` |
| a11y/lint/audit | 450 | 3 | `a11y.rs`, `audit.rs`, `wcag.rs` |
| Infra/other | 1,959 | 9 | `element.rs` (886 — `Element`/`LeafWidget`/`NodeContent`, must travel with the catalogue), `theme.rs`, `widget.rs` (`impl_common!`), `motion.rs`, `asset.rs`, `boundary.rs`, `macros.rs`, `design.rs`, `lib.rs` |

Two corrections to the modularity review's own framing, found while
breaking this down: `motion.rs`'s `spring()` helper is a **second**,
separate animation mechanism from the `AnimVal`/`PropAnim` engine that
actually lives in `app.rs` — a `lumen-app` extraction that moves only
`app.rs` leaves half of "the animation engine" behind in the widget crate.
And `design.rs` (a JSON-design-spec → `.lss` importer, 37 LOC) is a dev-tool,
not a widget or a runtime concern — it belongs in `lumen-cli`, not in
whichever crate inherits "everything else."

### Proposed layout

```
lumen-core, lumen-macros, lumen-layout, lumen-render, lumen-text   (unchanged)
lumen-style          (unchanged, + registry.rs for register_property, §5)
lumen-widgets         9,100 (catalogue) + element.rs + widget.rs + theme.rs
   ~10.5k LOC          + macros.rs + boundary.rs + asset.rs + motion.rs
                        (files regrouped by domain per SD2: overlay.rs,
                        pickers.rs, nav_chrome.rs, panes.rs, lists.rs —
                        replacing widgets_m1/m3/m4/extra/misc_w2)
lumen-app             app.rs's 4,613 LOC, unchanged content, new crate
   ~4.6k LOC           boundary. depends on core/render/layout/text/style/widgets.
lumen-toolkit         forms + nav + undo + i18n + system + tasks (948)
   ~1.4k LOC           + a11y/audit/wcag (450) — combined, not split further;
                        see rationale below. Depends on lumen-app (needs
                        App/BuildCx/Headless) + lumen-widgets.
lumen-shell-core       new. ~150-200 LOC (render_once + Session, §5).
lumen-shell(-android/-ios/-web)   unchanged crate boundaries, now depend on
                        lumen-shell-core instead of duplicating render_into/session glue.
lumen (facade)         re-exports core/layout/render/text/style/widgets/app/
                        toolkit; flat widget names only, no milestone tags.
lumen-agent, lumen-cli, lumen-test, skills-smoke   unchanged, + lumen-cli
                        gains design.rs (the JSON-design importer moves here).
```

**19 crates total** (16 today + `lumen-app` + `lumen-toolkit` + `lumen-shell-core`),
not the ~21 a fully-split toolkit/a11y would produce. **Why combine
toolkit+a11y into one crate rather than two**: both are small (948 + 450 =
1,398 LOC combined), both sit at the same dependency depth (above
`lumen-app`, needed by `lumen-cli`/`lumen-test`/real apps but not by the
widget catalogue itself), and the modularity review's own top-5 list never
separately costed an a11y-tooling split — it named app-toolkit as
"conceptually a layer above widgets" in the crate-by-crate assessment
(`03-modularity.md:233-238`) without proposing it as its own crate. Splitting
it further trades a two-crate-count increase for no boundary anyone has
asked to draw at build time; a single `lumen-toolkit` gets the actual
win (out of the 26k-LOC crate, out of the widget-catalogue's release
surface) at lower crate-proliferation cost. If a future consumer wants
a11y tooling without forms/nav/undo (e.g. a CI-only lint tool), that's the
signal to split it then — not a hedge worth taking now, pre-1.0, with zero
external consumers.

**Dependency chain**: `lumen-core → lumen-layout/lumen-render →
lumen-text/lumen-style → lumen-widgets → lumen-app → lumen-toolkit →
lumen(facade) → lumen-shell*`. Still acyclic (the review verified zero
cycles in the current graph; this proposal adds two new links,
`lumen-app→lumen-widgets` and `lumen-toolkit→lumen-app`, both downward,
neither closing a cycle).

**Migration cost**: `SD1`'s own estimate stands — "a new crate, a re-export
shim in `lumen-widgets` for one release, and the facade's `pub use
lumen_widgets::{app::FrameStats, App, ...}` becomes `pub use lumen_app::...`"
(`03-modularity.md:494-500`). The toolkit extraction is the same shape,
smaller: six files' worth of `mod` declarations move, `use crate::` paths
inside them become `use lumen_app::` / `use lumen_widgets::`, and the
facade's re-export block changes from `pub use lumen_widgets::{forms, nav,
undo, i18n, system}` to `pub use lumen_toolkit::{forms, nav, undo, i18n,
system}`. **Effort: `lumen-app` extraction ~3-4 days (matches the campaign's
own SD1 sizing); `lumen-toolkit` extraction ~1-2 days (smaller surface, no
`app.rs`-scale internal coupling to unwind); `lumen-shell-core` ~3-5 days
(§5). Total crate-restructuring effort: ~1.5-2 weeks**, dominated by
`lumen-app` because it is the file every other performance-track milestone
(OB2-4, CP1-CP6, AN1) also edits — hence tension #4 in §8 and its explicit
sequencing in §9.

---

## 8. Tensions

Five real conflicts, each named with its actual mechanism (not the vague
"API stability vs. performance" framing that's too coarse to act on) plus
the mitigation, where one exists.

**1. Lean-by-default modularity directly deletes the API's biggest
differentiator, in the configuration the campaign is pushing as the
default.** `ui.getDeps`/`ui.whatDependsOn` are `#[cfg(feature =
"snapshot")]`-only (`05-architecture.md:481-486`). M-E's lean-default flip
(the modularity/resource-usage fix) means any app built the recommended way
ships with **zero** agent introspection — the one property this note's §1
puts first, and the one no competitor framework has at all. The campaign
states this tension explicitly and accepts it without resolving it
(`zippy-dancing-allen.md:60-63`: *"Accepted, but stated."*). It is not
accepted here without a mitigation: A+ requires either (a) a distinct
`agent`-flavored lean profile (small binary, `snapshot` on, everything else
off — a third point on the feature matrix, not just "lean" and "full"), or
(b) shipping a dev-only introspection binary alongside the lean production
one. Either is cheap; leaving it unresolved is not compatible with an A+
API grade, because it means the framework's stated differentiator is
opt-out by default in exactly the build most users ship.

**2. `register_property`'s dispatch cost sits on the style resolution path,
which is already the subject of a D+ performance finding — softer in
practice than it first looks, once the actual call sites are read.**
`Style::apply()` has three call sites, and the one that matters for steady
state (`build_node`'s style resolution, `app.rs:3342`) is gated by a memo
cache keyed on `(id, classes, states, ty, overlay-hash)` — `apply()` only
runs on a **cache miss**, i.e. once per genuinely new style key, not per
node per frame (§5 has the full citation trail). So the real constraint on
`register_property` is "cheap per unique style key," a far weaker bar than
"cheap per frame." **Mitigation, and it's cheap given that bar**: keep the
existing closed `match` as the fast path for all built-in properties (zero
added cost), and only consult the registry in the `_ => {}` fallthrough arm
— third-party properties pay the indirection, built-in properties pay
nothing. Same "pay for what you use" shape as Compose's `Modifier.Node`
(dispatch only for the modifiers actually present on a node). The tension
is real in principle but close to fully mitigated once the actual call
frequency is accounted for — it is not a reason to decline the feature.

**3. Third-party extensibility structurally works against "exactly one way
to do each thing," which this note's §1 puts second.** A `register_property`
hook or a third-party `LeafWidget` necessarily creates code paths an AI
trained on the framework's own corpus has never seen — the opposite of
convergence. This is real and not fully resolvable by API design alone; the
mitigation is that Lumen already has the mechanism that makes it tractable
for *other* frameworks it isn't: the introspection surface. A third-party
property registered via `register_property` should be discoverable through
`ui.lint`/`ui.getStyles` (list registered custom properties, same as
built-in ones) rather than requiring an AI to have memorized it from
training data — turning "an extension I've never seen" into "an extension I
can query," which is not an option Flutter/SwiftUI/Compose's static-analysis
tooling gives an AI consumer at runtime. This is the one tension where
Lumen's actual differentiator is also the fix for a problem extensibility
otherwise makes worse.

**4. Crate-split work (`lumen-app` extraction, widget-file regrouping) and
the widget-catalogue/app-runtime bug fixes the campaign is also shipping
collide in the same files, and both want to land before 1.0.** The campaign
already names this for `SD1`: *"Last, because it conflicts mechanically
with every `app.rs` task above"* (`zippy-dancing-allen.md:321-322`). This
isn't a design tension, it's a sequencing one — but it is real and
under-costed: every milestone that touches `app.rs` (OB2-4, CP1-CP6,
AN1) has to land *before* the extraction, or the extraction has to
re-absorb their diffs, which is expensive churn either way. The path in §9
sequences this explicitly rather than leaving it to "last."

**5. API-freeze discipline (semver, `cargo-semver-checks` gating the facade,
`docs/api-audit-1.0.md:26-30`) and the modularity fixes both want to happen
"before 1.0," but only one of them is a one-way door.** Milestone-named
files leaking through the facade (`widgets_m3::DatePicker`) and the
`lumen-app` split are both flagged by the campaign itself as "a one-way
door if deferred past 1.0" (`03-modularity.md:389`, `zippy-dancing-allen.md:381`).
The API-freeze goal wants the facade to stop moving; the modularity goal
requires the facade to change (re-export paths move, `lumen_app::App`
replaces `lumen_widgets::app::App`). These are not actually in conflict —
they resolve by ordering: do every one-way-door modularity fix **before**
declaring the freeze, never after. The tension is real only if the two
milestones are scheduled in the wrong order, which is precisely what the
campaign's own rollback table (`zippy-dancing-allen.md:374-381`) already
warns about for `SD1`/`SD2` but doesn't sequence relative to a stated
freeze date. §9 fixes that.

---

## 9. The path — sequenced and costed

Built as an overlay on the approved campaign's M-A→M-F structure, not a
replacement — the campaign's perf/observability work is real and should
proceed; this path inserts the A+-only items (everything the campaign
declined) at the point their dependencies allow, and marks what must land
**before the 1.0 API freeze** (`docs/api-audit-1.0.md:26-30`,
`cargo-semver-checks`-gated) versus what is additive and can trail after.

**Ordering rule that drives the whole structure**: `SD1` (the `lumen-app`
extraction) and the new `lumen-toolkit` extraction are one-way doors that
must land before the freeze, but they collide with every task that edits
`app.rs` — which is most of the campaign's M-A–M-D work *and* most of this
note's new items (the mechanical `.lss` properties, several type-level
fixes). So: **do every `app.rs`-touching item first, the crate extraction
last, the freeze after that.** This is tension #4/#5 from §8, resolved by
sequencing rather than left as "last" with no ordering against a freeze
date, which is what the campaign's rollback table currently does.

### Phase 0 — cheap, parallel, no prerequisites (runs alongside campaign M-A)

| Item | Source | Effort | Touches |
|---|---|---|---|
| Fix the "39"→41 property count everywhere it's cited, before SD5.1 encodes it | §2 | 1h | docs, the `PARSE_ONLY_PROPERTIES` const itself |
| Type fixes #1 (`Copy` bound), #7 (`SharedStub`), #8 (`const fn` `srgb8`), #9 (`leaf` sizing) | §3 | ~1-2 days combined | `element.rs`, `widget.rs`, `color.rs` |
| `SignalKey<T>` (§6 Layer 1) — ships alongside the campaign's K1 (same area, non-overlapping: K1 touches `state.rs`'s slot/panic path, `SignalKey<T>` touches `BuildCx::signal`'s call convention) | §6 | ~2-3 days | `element.rs`, `state.rs` (new pub type only, no internal change) |
| Correct `.ai_docs/01-architecture.md:11`'s "`Widget` trait" → `LeafWidget` (stale, confirmed by grep) | §5 | 15m | docs |
| Correct `lumen-shell-android/src/lib.rs:3-6`'s stale "input not wired" comment (confirmed false — `imp.rs:118-224` handles it) | §5 | 15m | docs |
| Type fixes #3 (`Focusable`), #4 (`WidgetId`) | §3 | ~2-3 days combined | `element.rs`, widget constructors (mechanical find-replace) |

### Phase 1 — the mechanical/medium `.lss` properties (before `lumen-app` extraction, because they touch `app.rs`)

| Item | Effort | Prerequisite |
|---|---|---|
| 24 mechanical properties (§2: layout-tier via `LayoutStyle`, text-tier via existing parley fields) | ~1-2 days | none |
| 9 medium properties (grid track/line grammar, remaining text tier) | ~2-3 days | none |
| `cursor` (new field + winit hookup) | ~1 day | none |
| `filter: blur()` only (reuses existing blur pipeline) | ~0.5 day | none |
| **Deferred past 1.0, explicitly**: `filter`'s non-blur functions, `transform`'s hit-test-parity half, `z-index` — genuinely new engine work (§2's "hard" 3), not required for the A+ *API* grade (a documented, diagnosed gap is honest; an unfinished stacking-context/hit-test engine feature is not something 1.0 needs to gate on) | ~1.5-2 weeks | can trail 1.0 |

**Total Phase 1: ~1 week landed pre-1.0** (mechanical + medium + cursor +
blur), leaving only the genuinely hard render-engine work for after.

### Phase 2 — the wider type-level fixes (before the freeze; these are breaking)

| Item | Effort | Note |
|---|---|---|
| #6 `Interactive` capability types, replacing `actions: Vec<Action>` + loose handler fields | ~1-1.5 weeks | Touches every widget file; subsumes `W0106`/`audit_actions()` |
| #10 `ScopeCx` split (root vs. scoped signal addressing become different types) | ~1 week | Touches every `cx.scope(...)` call site across the widget catalogue and examples |
| #5 `Element<Semantic\|Elided>` marker type | design spike first (~2-3 days), then ~1-1.5 weeks if the spike says go | The one item this note explicitly declines to pre-commit past a spike — see §3's cost caveat |
| `register_property` phase 1 (§5) | ~3-5 days | Independent of the above; can run in parallel |
| `SD3` (real free-function shims), `SD4` (fold `audit_actions`/re-export `theme`/`Shadow`), `SD5.0-5.5` (`.lss` diagnostics for the remaining hard properties) | campaign-scoped, unchanged | Runs in parallel with the above; no new cost from this note |

### Phase 3 — crate restructuring (last among `app.rs`-touching work, still pre-freeze)

| Item | Effort | Prerequisite |
|---|---|---|
| `lumen-app` extraction (`SD1`) | ~3-4 days | **All of Phase 1 and Phase 2's `app.rs`-touching items land first** — this is the ordering fix for tension #4 |
| `lumen-toolkit` extraction (new, §7) | ~1-2 days | after `lumen-app` (depends on it) |
| `SD2` (widget-file regroup, retire milestone names from facade) | campaign-scoped | independent — can run any time before the freeze, no `app.rs` collision |
| `lumen-shell-core` (§5) | ~3-5 days | independent — internal refactor, no public-API collision |

### Phase 4 — the 1.0 API freeze

Declared only after Phases 0-3 are in. At this point: no bare string keys
are the *taught* pattern for flat state (`SignalKey<T>` + typed enums for
scoped state); no widget can declare an action it doesn't implement (type,
not audit); no facade re-export carries a milestone name; `lumen-app`/
`lumen-toolkit` exist as their own versioned boundary; `register_property`
and `lumen-shell-core` exist; the mechanical/medium `.lss` property gap is
closed and the remainder is diagnosed, not silent.

### Phase 5 — additive, explicitly OK to trail past 1.0

Widget-completeness finishing (remaining arrow-key nav for
`Combobox`/`Tabs`/`Menu`/2-D grids, `RadioGroup`, `SegmentedControl`/
`ListTile`/`Rating`/`Breadcrumb` — all additive per §4); `filter`'s
non-blur functions, `transform` hit-test parity, `z-index` (§2's hard 3);
`register_property` phase 2 (`Value::Custom`, §5); K2's optional
`#[derive(StateKey)]` (§6, "kill if unearned"); competitive benchmarking
against SwiftUI/Compose/Flutter/Dioxus/Leptos as an ongoing scorecard, not
a one-time gate.

### Total cost estimate

| Phase | Calendar (1 engineer, sequential) | Parallelizable? |
|---|---|---|
| 0 | ~1.5 weeks | Yes — independent items, can run 2-3 wide |
| 1 | ~1 week | Partially — property groups are independent of each other |
| 2 | ~3-4 weeks | Partially — `Interactive`/`ScopeCx`/`register_property` are independent of each other, not of Phase 0/1 |
| 3 | ~1-1.5 weeks | No — strictly sequential (`lumen-app` before `lumen-toolkit`) except `SD2`/`lumen-shell-core` which parallelize with everything |
| **Total to 1.0 freeze** | **~7-9 weeks sequential, ~5-6 weeks with 2-3 engineers on independent tracks** | |
| 4 (freeze) | — | milestone, not work |
| 5 (post-1.0) | ongoing | fully parallel, no deadline |

This is on the same order of magnitude as the campaign's own M-A-through-M-F
estimate for the performance/observability work it already scoped — **A+ on
these two axes is not a second project, it is roughly 6-9 additional weeks
layered onto the campaign already approved**, concentrated in Phase 2 (the
type-level API redesign) because that is the one place this note asks for
something the campaign never scoped at all, not a bigger version of
something it already planned.
