# PULSE — chronograph stopwatch

A standalone Lumen example: a luminous circular-dial stopwatch with two themes
(**Eclipse**, dark; **Daybreak**, light) that toggle live. Control colours were
tuned with the design-analysis APCA contrast tool (`Headless::contrast_report`);
the test suite gates every label at APCA `|Lc| >= 60`.

```sh
cargo run -p chrono-stopwatch     # writes /tmp/chrono-*.png + prints contrast reports
cargo test -p chrono-stopwatch    # legibility gate + behaviour
```

## Visual specification (verify the render against this)

### Overall composition
- A single rounded-rectangle **device body** floats centred on a flat full-window
  background. The body is `360 × 468` px, corner radius `30`, with a large soft
  drop shadow offset downward (`dy 22`, `blur 48`).
- Inside, three rows stacked and centred with even spacing: **header**, **dial**,
  **controls**.
- **Header** spans the body width: brand wordmark `PULSE` (uppercase, bold,
  ~15px) on the **left** in the accent colour; a small rounded **theme-toggle**
  pill on the **right** whose label is the *other* theme — `LIGHT` in dark mode,
  `DARK` in light mode.
- **Dial**: a `280 × 280` circular instrument (see below).
- **Controls**: two pills, horizontally centred, ~12px apart — the primary
  **Start/Stop** toggle on the left, a quieter **Reset** on the right. Labels are
  centred within each pill (16px, semibold), corner radius `13`.

### The dial (centre to edge)
1. A flat **recessed face** disc (slightly different tone from the body).
2. An oversized **readout** dead-centre: `MM:SS`, ~66px, heavy (800 weight),
   tabular — e.g. `00:00` idle, `01:27` after 87.5s.
3. Directly **above** the readout, a small uppercase **status** word: `READY`
   (idle, zero), `RUNNING` (active), or `PAUSED` (stopped with time on the clock).
4. Directly **below** the readout, a smaller **centiseconds** chip in the accent
   colour: `.00`, `.50`, etc. (~22px).
5. A thin **bezel track** ring near the rim, with **60 tick marks** — every 5th
   tick is longer and brighter (the 12 "hour" majors).
6. A **progress arc** stroked over the track, ~10px wide, starting at **12
   o'clock** and sweeping **clockwise** proportional to seconds-within-the-minute
   (one full revolution per 60s). At `01:27.50` it covers ~46% (ends low-right,
   ~5:30 position).
7. A **glowing head dot** rides the tip of the arc: a bright solid dot inside a
   larger translucent halo of the same hue. At idle it sits at 12 o'clock.

### Eclipse (dark theme)
- Page `#0c0f15`; body `#161b24`; recessed face `#10141c`.
- Bezel track `#232b39`; minor ticks `#2b3543`; major ticks `#46546b`.
- Arc **teal** `#2dd4bf`; head `#5eead4` with a teal halo.
- Readout near-white `#eef3f8`; status `#b3bdcc`; centiseconds & brand teal
  `#2dd4bf`.
- **Start** pill: teal fill, near-black text. **Stop** pill: crimson `#e11d48`
  fill, near-white text. **Reset**: dark slate `#1c2330` fill, light text.

### Daybreak (light theme)
- Page warm paper `#f5f1e8`; body `#fffdf8`; recessed face `#f6f0e4`.
- Bezel track `#e7dfd0`; minor ticks `#d9d0bf`; major ticks `#b7a98f`.
- Arc **vermilion** `#e8590c`; head `#ff7a33` with a vermilion halo.
- Readout near-black ink `#1c1d22`; status `#756b58`; centiseconds `#c2490a`;
  brand `#b8480a`.
- **Start** pill: ink `#1c1d22` fill, paper text (confident dark button).
  **Stop** pill: burnt-orange `#b8480a` fill, near-white text. **Reset**: warm
  grey `#efe8da` fill, dark text.

### Behaviour
- **Start** begins integrating time and animates the arc + readout; the pill
  becomes **Stop**. **Stop** pauses (status → `PAUSED`). **Reset** zeroes the
  readout and stops. The header pill swaps the whole palette instantly.
