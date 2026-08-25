# Experiment: a `Widget` trait, and what it costs

**Branch** `exp/widget-trait` · **Baseline** `main` @ `0b2ec7f` · **Run** 2026-08-25
**Box** Linux 6.8, glibc malloc, `--release` (`lto = "thin"`, `codegen-units = 1`)

## The question

Extensibility, and keeping each widget down to what it actually needs.

Every typed widget was a newtype over a fully-built `Element`:

```rust
pub struct Button { el: Element }
```

Two consequences. Every widget value was `size_of::<Element>()` — **1072
bytes** — whatever it actually needed; and there was no *contract* a widget
satisfied, only a convention (`::new()` builds a node, `From<W> for Element`
hands it over) plus a `pub(crate)` macro nobody outside the crate could use.

The experiment replaces both with a trait and deferred lowering. A widget stores
only its own fields and materializes the node once, when it is handed to the
tree:

```rust
pub trait Widget: Sized {
    fn build(self) -> Element;
}
```

The question this file answers: **does that cost more than the extensibility and
the smaller widget values are worth?**

## Dispatch: this is a lowering seam, not a `dyn` tree

`Widget` is `Sized`-bound and consumed by value. `impl_widget!` generates
`From<W> for Element` as a direct call to `Widget::build`, monomorphized.
**There is no `Box<dyn Widget>` anywhere**, and `Element::children` is still
`Vec<Element>`. That is why allocation counts per node are unchanged — the
trait is a compile-time contract for *lowering into* `Element`, not a runtime
one for *replacing* it.

The other design — the framework retaining a `dyn Widget` tree and calling back
into it to measure, paint and route events, as iced and Flutter do — is a
different experiment with a different cost profile (one allocation and one
vtable indirection per node, and `Element` stops being the interchange format).
**It is not what was measured here.**

## What was done

All **57** typed widgets converted; the old `impl_common!` deleted.
`From<W> for Element` survives, so ordinary call sites compile unchanged.

`element()` / `element_mut()` could not survive — there is no built node to
borrow before `build` — so the nine call sites (all in `widget_gallery`) lower
first and tweak the `Element`. One turned out never to have needed the escape
hatch: `TextField::width` already existed.

**1069 workspace tests pass; clippy clean.**

### Extensibility, verified rather than assumed

`tests/third_party_widget.rs` defines widgets *outside* the framework's own
files and asserts four properties:

1. A foreign type implementing `Widget` composes with the built-ins in a live
   tree and reaches semantics.
2. A foreign widget can **hold built-in widget values unbuilt** (`Label`,
   `Button`) and lower them itself — composition, not just conversion.
3. With `impl_widget!` it **inherits the whole universal vocabulary** —
   `.id()`, `.class()`, `.background()`, `.style()`, `.css()`, `.disabled()` —
   including the disabled dimming, which a hand-rolled widget would have to
   reimplement and would probably forget.
4. It is addressable by stable id in a live tree, like any built-in.

Property 3 only holds because `impl_widget!` is `#[macro_export]`ed with every
path in its expansion routed through `$crate`, and `Common` has a public
builder API and a public `apply`. **In the first pass all of that was
`pub(crate)`**, which meant a foreign widget could implement `Widget` and
inherit none of the six modifiers — the trait would have bought a third party
almost nothing. That was the single most important defect in the experiment,
and it was invisible until the extensibility claim was actually tested.

### Eight widgets could not defer, for structural reasons

`Sheet`, `Drawer`, `RichTextEditor`, `FindReplaceBar`, `VirtualList`,
`DataGrid`, `Tree`, and the dropdown panels of `Combobox` / `PickList` keep a
built `Element`:

* **Borrowed generic inputs.** `render: impl Fn(usize) -> Element`,
  `rows: &[TreeRow]` are bound to the constructor call, and a widget value
  outlives it. Storing them means boxing per widget, or a generic
  `VirtualList<F>` that cannot sit in a `Vec<Element>` beside its siblings.
* **The build context reaches deeper than the constructor.**
  `VirtualList::memoized` wraps each row in `cx.scope_with_deps`; `Sheet` needs
  the window size; `FindReplaceBar` needs a search over the editor's live text.

**Deferral reaches exactly as far as ownership and the build context allow.**

## Results

### Carrying only what you need

| | eager | deferred | |
|---|---:|---:|---|
| `ProgressBar` | 1072 B | 136 B | **7.9× smaller** |
| `CheckBox` | 1072 B | 144 B | 7.4× |
| `Button` | 1072 B | 152 B | 7.1× |
| `Card` | 1072 B | 160 B | 6.7× |
| `Chip` | 1072 B | 176 B | 6.1× |
| `Container` | 1072 B | 184 B | 5.8× |
| `Label` | 1072 B | 232 B | 4.6× |

Under the eager model these were all *exactly* 1072 bytes, because they were all
exactly an `Element`. Now each one's size is a fact about the widget.

### Allocations — where deferral is free, and where it is not

Per 1000 buttons, by modifier:

| | eager | deferred |
|---|---:|---:|
| no modifier | 3001 / 1056 KiB | 3001 / 1056 KiB |
| `.id("btn")` | 3001 / 1056 KiB | 3001 / 1056 KiB |
| `.class("x")` | 5001 / 1157 KiB | 5001 / 1157 KiB |
| `.style(..)` | 3001 / 1056 KiB | **4001 / 1306 KiB** |

`.style()` costs one extra allocation, and it is **inherent to deferral**: an
eager `.style(s)` writes into a `LayoutStyle` the node already owns, while a
deferred one has nowhere to put it until `build` and must either box it or carry
256 bytes on every widget that never sets it. Boxing is the right side of that
trade, but it is a real cost and not an artifact.

Everything else is at parity, including the whole-frame figures:

| | eager | deferred |
|---|---:|---:|
| allocations, 500-row changed frame | 60 779 | 60 779 |
| RSS for the 500-row app | +18 644 KiB | +18 712 KiB |
| `examples/hello` release binary | 8 333 072 B | 8 333 776 B |

> **This table was wrong in the first pass.** It reported "identical" on the
> strength of benchmarks that never called `.class()` or `.style()`. Measured
> properly, `.class()` cost **+2 allocations and +1.35 MB per 1000 widgets** —
> because `Common` boxed all three escape hatches behind one `Option<Box<Rare>>`
> with `LayoutStyle` and `Style` inlined, a ~1.3 KB record allocated in full to
> store one string. Fixing it (fields boxed individually; `apply` *moves* the
> class vector instead of `extend`ing into a fresh one) brought `.class()` back
> to byte-for-byte parity and improved the 500-row frame from +3.5% to +0.5%.

### Frame time — drift-controlled

Baseline was measured twice, on either side of the experiment, so machine drift
is visible rather than assumed. Where drift exceeds the effect, the row is
reported as inconclusive rather than quietly used.

| bench | baseline | experiment | self-drift | change | verdict |
|---|---:|---:|---:|---:|---|
| `construct/button_1k` | 188.4 µs | 136.9 µs | −0.1% | **−27.3%** | real win |
| `construct/label_1k` | 115.6 µs | 109.5 µs | −1.9% | **−5.3%** | real win |
| `construct/container_1k` | 254.9 µs | 281.5 µs | −2.7% | **+10.5%** | real loss |
| `construct/mixed_1k` | 605.0 µs | 618.9 µs | −1.9% | +2.3% | marginal |
| `frame/widget_rows_100` | 3.52 ms | 3.52 ms | −0.4% | **+0.0%** | wash |
| `frame/widget_rows_500` | 15.61 ms | 15.69 ms | −1.4% | **+0.5%** | wash |
| `construct/card_1k` | — | 230.9 µs | +8.9% | — | drift > effect |
| `construct/chip_1k` | — | 233.2 µs | −14.3% | — | drift > effect |
| `construct/progress_1k` | — | 261.2 µs | −13.8% | — | drift > effect |

**Frame time is a wash.** Both end-to-end frame benchmarks land inside the
baseline's own run-to-run variation.

### Compile cost

| | eager | deferred |
|---|---:|---:|
| `lumen-widgets` incremental release rebuild | 7.0 s | 10.2 s (**+45%**) |
| `lumen-widgets` source | 12 148 lines | 13 718 lines (**+13%**) |

This is the one unambiguous price.

## Where the per-widget time goes

Deferral wins where modifiers would otherwise **redo or undo** work, and loses
where they were already cheap:

* `Button` **−27.3%** — `.ghost()` overwrites the accent fill `new()` had just
  written, then re-reaches through `NodeContent` for the `TextStyle`. Deferred,
  the emphasis is a one-byte tag resolved once.
* `Label` **−5.3%** — five typography modifiers, each re-matching
  `NodeContent::Text` to borrow the style back out. Deferred, they are field
  writes.
* `Container` **+10.5%** — its modifiers were *already* single-field writes into
  a built `LayoutStyle`. Deferring means storing eight fields and copying them
  into a `LayoutStyle` at build: a store-then-copy round trip the eager version
  never paid.

The same logic explains `TextInput` (whose `refresh()` re-derived the shown
string, caret mapping, label and value from *every* modifier), `Slider`
(`.step()` rebuilt three `Rc` closures `::new()` had just allocated), and `Menu`
(`.on_select()` rebuilt every item).

## The correctness dividend

Deferring lowering removed four real order dependencies, none of which were the
goal:

1. **`.disabled(true)` dimmed immediately**, so a later `.ghost()` silently
   un-dimmed the button.
2. **`Chip::selected` recoloured `children.first_mut()`** — the *icon* whenever
   `.icon()` ran first.
3. **`Popover::side` reached into `children.get_mut(1)`** and did nothing at all
   while the popover was closed.
4. **`Slider` formatted its accessible value in `::new()`** with the default
   step, so `.step(0.01)` left it rounded to whole numbers.

`Avatar::image` also stops shaping initials it is about to discard, and
`Toast::auto_dismiss` stops building a toast it is about to replace with
`Element::default()`.

## Verdict against the stated goals

**Extensibility: delivered, and it needed the export fix to be real.** A foreign
crate can define a widget that composes with built-ins, holds them unbuilt, and
inherits the full modifier vocabulary including the disabled dimming. None of
that was possible before — the shared vocabulary lived in a `pub(crate)` macro.

**Carry only what you need: delivered.** 1072 bytes flat becomes 136–232 bytes,
and the number now means something per widget.

**Cost: acceptable on runtime, real on build.** Frame time is inside noise
(+0.0% / +0.5%). Allocations are at parity except `.style()`, which pays one
allocation for not carrying 256 bytes everywhere. The price is **+45% compile
time and +13% source** — paid by the framework's own build, not by consumers.

**So: adopt it for the reasons it was proposed, not for speed.** The one
runtime caveat worth carrying forward is `Container`, +10.5%: widgets whose
modifiers are already direct writes into `LayoutStyle` are made slower by
deferral, and `Container` is among the most-instantiated widgets in any real
tree. If anything here deserves a second pass, it is giving `Container` a
stored `LayoutStyle` instead of eight loose fields, and re-measuring.

## Notes

This branch is an experiment and is **not** proposed for merge as-is, so the
`.ai_docs` specs and task graph are deliberately untouched (AGENT.md
doc-currency governs commits that change shipped public behavior). This file is
the record.

Instruments, byte-identical in both trees:
* `benches/benches/widgetcost.rs` — construction and end-to-end frame timing.
* `benches/src/bin/widgetprobe.rs` — sizes, allocation counts by modifier, RSS.
* `crates/lumen-widgets/tests/third_party_widget.rs` — the extensibility claims.
