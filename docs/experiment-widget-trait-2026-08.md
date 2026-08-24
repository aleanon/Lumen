# Experiment: a `Widget` trait, and what it costs

**Branch** `exp/widget-trait` · **Baseline** `main` @ `0b2ec7f` · **Run** 2026-08-25
**Box** Linux 6.8, glibc malloc, `--release` (`lto = "thin"`, `codegen-units = 1`)

## The question

Every typed widget was a newtype over an `Element` built inside its `::new()`:

```rust
pub struct Button { el: Element }
```

So every widget value was `size_of::<Element>()` — **1072 bytes** — and each
modifier mutated an already-materialized node. The repo's own `buildscale.rs`
already suspected this shape: *"3000 nodes × 1072 B is 3.2 MB of Elements
alone"* against a 2 MB L2.

The experiment replaces it with deferred lowering. A widget stores only its own
fields and materializes the node once, at the point it is handed to the tree:

```rust
pub trait Widget: Sized {
    fn build(self) -> Element;
}
```

The universal modifiers (`id`, `class`, `background`, `style`, `css`,
`disabled`) move into a `Common` record folded on *after* the widget builds.

## What was done

All **57** typed widgets converted; `impl_common!` deleted. `From<W> for Element`
still exists (it calls `Widget::build`), so ordinary call sites —
`Button::new("x").into()` — compile unchanged.

`element()` / `element_mut()` could not survive: there is no built node to borrow
before `build`. The nine call sites (all in `widget_gallery`) lower first and
tweak the `Element`. One of them turned out never to have needed the escape
hatch — `TextField::width` already existed.

**1064 workspace tests pass; clippy clean.**

### Eight widgets could not defer, for a structural reason

`Sheet`, `Drawer`, `RichTextEditor`, `FindReplaceBar`, `VirtualList`,
`DataGrid`, `Tree`, and the dropdown panels of `Combobox` / `PickList` keep a
built `Element`. Two causes, both structural:

* **Borrowed generic inputs.** `render: impl Fn(usize) -> Element`,
  `rows: &[TreeRow]` are bound to the constructor call. A widget value outlives
  that call, so storing them means boxing per widget, or a generic
  `VirtualList<F>` that cannot sit in a `Vec<Element>` beside its siblings.
* **The build context reaches deeper than the constructor.**
  `VirtualList::memoized` wraps each row in `cx.scope_with_deps`; `Sheet` needs
  the window size; `FindReplaceBar` needs a search over the editor's current text.

**Deferral reaches exactly as far as ownership and the build context allow.**

## Results

### Memory — the one unambiguous win

| | eager | deferred | |
|---|---:|---:|---|
| `Button` | 1072 B | 120 B | **8.9× smaller** |
| `Label` | 1072 B | 200 B | 5.4× |
| `Container` | 1072 B | 152 B | 7.1× |
| `Card` | 1072 B | 128 B | 8.4× |
| `Chip` | 1072 B | 144 B | 7.4× |
| `CheckBox` | 1072 B | 112 B | 9.6× |
| `ProgressBar` | 1072 B | 104 B | **10.3×** |

And the things that did **not** move:

| | eager | deferred |
|---|---:|---:|
| allocations to build 1000 buttons | 5001 | 5001 |
| allocations, 500-row changed frame | 60 779 | 60 779 |
| RSS for the 500-row app | +17 208 KiB | +17 296 KiB |
| `examples/hello` release binary | 8 333 072 B | 8 333 776 B |

Allocation counts are *identical*, which is the expected result: the allocations
belong to the `Element`, not to when it is built. Deferring moves the work, it
does not remove it.

### Frame time — drift-controlled

Baseline was run **twice**, on either side of the experiment, so machine drift is
visible rather than assumed. `drift` is baseline-B against baseline-A: where it
is large, the benchmark is not trustworthy at these effect sizes.

| bench | base A | base B | drift | baseline | experiment | change | verdict |
|---|---:|---:|---:|---:|---:|---:|---|
| `construct/button_1k` | 188.5 µs | 188.3 µs | −0.1% | 188.4 µs | **134.1 µs** | **−28.8%** | real win |
| `construct/label_1k` | 116.8 µs | 114.5 µs | −1.9% | 115.6 µs | 107.4 µs | **−7.1%** | real win |
| `construct/container_1k` | 258.3 µs | 251.4 µs | −2.7% | 254.9 µs | 293.7 µs | **+15.2%** | real loss |
| `construct/mixed_1k` | 610.8 µs | 599.2 µs | −1.9% | 605.0 µs | 611.0 µs | +1.0% | wash |
| `frame/widget_rows_100` | 3.52 ms | 3.51 ms | −0.4% | 3.52 ms | 3.53 ms | +0.5% | wash |
| `frame/widget_rows_500` | 15.72 ms | 15.50 ms | −1.4% | 15.61 ms | 16.15 ms | +3.5% | slight loss |
| `construct/card_1k` | 217.9 µs | 237.3 µs | +8.9% | — | — | — | inconclusive |
| `construct/chip_1k` | 248.6 µs | 212.9 µs | −14.3% | — | — | — | inconclusive |
| `construct/progress_1k` | 281.4 µs | 242.5 µs | −13.8% | — | — | — | inconclusive |

### Compile cost

| | eager | deferred |
|---|---:|---:|
| `lumen-widgets` incremental release rebuild | 7.0 s | 10.2 s (**+45%**) |
| `lumen-widgets` source | 12 148 lines | 13 718 lines (**+13%**) |

## What the numbers mean

**1. The premise was wrong.** The case for this refactor was that a builder chain
takes `self` by value and returns `Self`, so `Button::new(..).ghost().on_press(..)
.id(..)` moves a kilobyte per link. In a release build **those moves do not
happen** — LLVM constructs the node in place and never materializes the
intermediate. The proof is `Container`: it shrank **7.1×** and got **15.2%
slower**. If the moves had been real, the widget with five modifier calls and the
largest payload would have won the most.

**2. Deferral wins exactly where modifiers redo or undo work.**

* `Button` −28.8%: `.ghost()` overwrites the accent fill `new()` had just
  written, then re-reaches through `NodeContent` for the `TextStyle`. Deferred,
  the emphasis is a one-byte tag resolved once.
* `Label` −7.1%: five typography modifiers, each of which re-matched
  `NodeContent::Text` to borrow the style back out. Deferred, they are field
  writes.
* `Container` +15.2%: its modifiers were *already* single-field writes into a
  built `LayoutStyle`. Deferring means storing eight fields and then copying
  them into a `LayoutStyle` at build — a store-then-copy round trip the eager
  version never paid.

The same logic predicts wins for `TextInput` (whose `refresh()` re-derived the
shown string, caret mapping, label and value from *every* modifier), `Slider`
(`.step()` rebuilt three `Rc` closures `::new()` had just allocated), and `Menu`
(`.on_select()` rebuilt every item).

**3. At the app level it does not matter.** A 500-row frame builds 1500 typed
widgets. At `mixed_1k`'s rate that is ~0.9 ms of a 15.6 ms frame — under 6%.
Even the −28.8% on `Button` is worth ~0.3% of a frame. The typed-widget layer is
not where frame time goes; layout, text shaping and paint are.

## The correctness dividend

Deferring lowering removed four real order dependencies, none of which were the
goal:

1. **`.disabled(true)` dimmed immediately**, so a later `.ghost()` silently
   un-dimmed the button. The dimming is now applied to the finished node.
2. **`Chip::selected` recoloured `children.first_mut()`** — which was the *icon*
   whenever `.icon()` ran first. It now recolours the label by name.
3. **`Popover::side` reached into `children.get_mut(1)`** and did nothing at all
   when the popover happened to be closed. It is a stored choice.
4. **`Slider` formatted its accessible value in `::new()`** with the default
   step, so `.step(0.01)` left the value rounded to whole numbers.

`Avatar::image` also stops shaping initials it is about to discard, and
`Toast::auto_dismiss` stops building a toast it is about to replace with
`Element::default()`.

## Recommendation

**Do not adopt this for performance.** It is a wash on frame time (+0.5% and
+3.5% on the two frame benches), costs +45% compile time and +13% source, and
its one large win is worth ~0.3% of a frame.

**It is defensible on other grounds** — order-independence, four fixed bugs, and
a model where a widget's fields are visible instead of smeared into a node. That
is an API-design argument, and it should be argued on those terms with the
compile-time cost stated, not smuggled in as an optimization.

**If any part is worth taking on its own**, it is the handful of widgets whose
modifiers demonstrably redo work — `Button`, `Label`, `TextInput`, `Slider`,
`Menu`. They can each adopt the pattern without a trait, and without the other
52 paying for it.

**The genuine memory finding stands and points elsewhere.** `Element` is 1072
bytes and that *is* worth attacking — but at the `Element` itself (the `EL`
bundling idea `buildscale.rs` describes), not at the widget wrapper, which is
transient and which the optimizer already erases.

## Notes

This branch is an experiment and is **not** proposed for merge, so the `.ai_docs`
specs and the task graph are deliberately untouched (AGENT.md doc-currency
applies to commits that change shipped public behavior). This file is the record.

Instruments added, identical in both trees:
* `benches/benches/widgetcost.rs` — construction and end-to-end frame timing.
* `benches/src/bin/widgetprobe.rs` — sizes, allocation counts, RSS.
