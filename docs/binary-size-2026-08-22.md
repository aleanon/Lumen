# Why the Lumen binary is 5 MB bigger than iced's (2026-08-22)

BENCH4 measured a lean Lumen app at **15.9 MB** against iced's **10.7 MB** and
Xilem's **10.8 MB**. This is where the 5.3 MB goes. Same workspace, same
release profile (`lto`, `codegen-units=1`, `strip`, `panic=abort`), same box.

## The answer in one line

**A 3.62 MB ICU CJK/Thai segmentation dictionary — 69% of the gap — that a
Latin-only build still carries.**

## Accounting

Section sizes first, because they say immediately that this is not a code
problem:

| | `.text` | `.rodata` | total |
|---|---:|---:|---:|
| Lumen (lean) | 9.60 MB | **5.05 MB** | 15.95 MB |
| iced | 8.40 MB | 0.85 MB | 10.66 MB |
| Xilem | 8.27 MB | 1.20 MB | 10.80 MB |

Code is within 15%. **Read-only data is 6× bigger**, and the strings in it are
the same size in both (0.38 MB vs 0.37 MB) — the difference is binary tables:
3.91 MB against 0.33 MB.

Masking out strings and zero-fill leaves **one 3.605 MB blob**, whose header
reads `1e 63 6a 64 69 63 74` — ASCII `cjdict`. That is ICU4X's dictionary for
languages that do not put spaces between words.

| component | delta vs iced | where it comes from |
|---|---:|---|
| ICU `cjdict` dictionary | **+3.62 MB** | `icu_segmenter` ← parley `complex-scripts` |
| accesskit (unix + consumer) | +0.54 MB | iced ships no accessibility |
| lumen crates (app/render/shell/style) | +0.52 MB | the framework itself |
| embedded fonts | +0.50 MB | Latin subset 0.34 + symbols 0.16 (ADR-005: no system fonts) |
| taffy | +0.26 MB | iced hand-rolls its layout |
| tiny-skia (over iced's use of it) | +0.18 MB | Lumen ships a CPU renderer *and* wgpu |
| misc (zbus, naga, hashbrown, …) | +0.24 MB | |
| iced-only crates | −0.41 MB | `iced_winit`, `iced_renderer`, `iced_wgpu`, `cosmic_text` |
| | **≈ +5.3 MB** | |

Everything except the dictionary is a feature Lumen has and iced does not, or
the framework's own code. Those are defensible. The dictionary is not, in a
build that has already decided to embed only the Latin font subset.

## What the dictionary actually buys

`parley`'s `complex-scripts` feature switches its line and word segmenters from
`new_for_non_complex_scripts` to `new_dictionary`. The workspace manifest
justified enabling it like this:

> `complex-scripts` bundles the ICU dictionary line-break models (CJK/Thai/…);
> without it parley panics ("no segmentation model for language: ja") on CJK.

**That comment is wrong on both counts.** Measured with
`cargo run -p lumen-text --example cjk_probe --features pan-unicode`, wrapping
at 160 px:

| | dictionary ON | dictionary OFF |
|---|---|---|
| ja | 160.0 × 62.4 px | 160.0 × 62.4 px |
| zh | 160.0 × 62.4 px | 160.0 × 62.4 px |
| **th** | **127.6 × 41.6 px** | **222.8 × 20.8 px — overflows, no wrap** |
| latin | 151.6 × 62.4 px | 151.6 × 62.4 px |

It does **not panic** — ICU logs `No segmentation model for language: ja` to
stderr and carries on. And **Japanese and Chinese wrap identically**, because
CJK has line-break opportunities between most characters without any
dictionary at all. What genuinely breaks is **Thai** — and by the same
mechanism Lao, Khmer and Burmese, which are the other scripts in `cjdict`.

The full workspace test suite passes with the feature off.

**Not measured, and a real cost:** the same data backs `WordSegmenter`, so
word-granularity cursor movement and double-click selection in CJK and Thai
degrade even where line breaking does not.

## Resolution — implemented 2026-08-23

`complex-scripts` is now a `lumen-text` feature, forwarded through every
consumer (`lumen-app`, `lumen-widgets`, `lumen-shell`, `lumen-agent`, the
facade and the four platform shells) exactly the way `pan-unicode` already was,
and **present in each of their `default` sets** — so a full build is byte-for-
byte what it was.

| build | before | after |
|---|---:|---:|
| `hello` (default) | 7.7 MB | 7.7 MB |
| `hello` (pan-unicode) | 22.2 MB | 22.2 MB |
| **lean-app** | 6.9 MB | **3.3 MB** |
| **win-app** (what a user ships) | 14.0 MB | **10.4 MB** |
| **nogpu-app** | 10.9 MB | **7.3 MB** |

The lean profiles were not opted out by hand: they already pass
`default-features = false`, so making the dictionary a default rather than a
hardcoded parley feature dropped it from all three at once. **The shipped
windowed profile lost 3.6 MB and the scaffolded lean app halved.**

That is coherent rather than a regression, and for the same reason the split
was proposed: those profiles embed only the Latin+symbols face, so they could
not draw a Thai glyph even with perfect Thai line breaking. An app that
registers a wider face at runtime with `App::font(bytes)` and needs the
segmentation turns the feature back on.

Size-gate ceilings were re-tightened (lean 8 → 5 MB, windowed 16 → 12, no-GPU
13 → 9) so the saving cannot be given back silently.

**The stderr concern was wrong and needed no fix.** `icu_provider` aliases its
`warn` to `eprintln` only under `debug_assertions`; with the feature off a
release binary is silent, verified by running one. Debug builds do print, which
is developer-visible and harmless.

## Reproducing

```sh
# section split
size -A -d <binary>

# find the blob
cargo build --release            # with strip = false in the profile
python3 - <<'EOF'                # mask strings + zero-fill, report what is left
EOF

# the behavioural trade-off
cargo run -p lumen-text --example cjk_probe --features pan-unicode
```
