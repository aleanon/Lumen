# BENCH5 harness contract

Every harness in this directory measures the SAME thing so the rows compare.

* **Content** — `N` rows in a vertical container; row `i` reads `row <i>`.
* **Viewport** — 400 x 800 logical px.
* **Modes**
  * `point` — row 0's text is replaced each frame (large tree, tiny change).
  * `churn` — *every* row's text is replaced each frame (nothing is reusable).
* **Timing** — minimum of `ITERS` timed iterations after 20 warm-up ones. The
  minimum is the least-interfered sample; on this box a background process at
  100% CPU has moved a mean by 38% while leaving the minimum alone.
* **Stages** — each harness reports its own pipeline split, cumulative, so the
  stage rows always sum to the total rather than to something else.
* **Memory** — `VmRSS` at three points and `VmHWM` (peak) at the end, all read
  from `/proc/self/status`. Same source and units in C, C++ and Rust, so the
  numbers are directly comparable; `rss.built - rss.base` is the cost of the
  N-row tree itself, isolated from the toolkit's fixed footprint.

Output is `key<TAB>value` lines so `run.sh` can assemble the tables.
