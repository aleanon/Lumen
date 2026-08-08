# CP4 (real ARM measurement) — blocked on hardware, 2026-08-08

## Why this exists

`docs/plan-incremental-path.md:217-237` makes CP4 a gate, not a footnote: the
retained-arena decision (CP5) is supposed to be taken with an ARM number in
hand, and `docs/results-node-cost-n0.md:182` says the same. The Phase-0 campaign
scheduled it early for exactly that reason, and because the 2026-08 resource
review graded mobile **D** partly because nothing has ever been measured there.

## Status: cannot be taken on this machine

| Requirement | State |
|---|---|
| Android SDK / build-tools / platform-34 | ✅ present |
| NDK 26.3.11579264 + `cargo-ndk` | ✅ present |
| `adb`, `emulator` | ✅ present |
| **arm64 system image** | ❌ none installed — only `x86_64` |
| **Physical ARM device** | ❌ none attached |
| Existing AVDs (`lumen34`, `guix_calc`) | both `google_apis/x86_64` |

## Why the obvious workarounds are wrong, not merely inconvenient

**An arm64 system image would run under QEMU instruction emulation on an x86_64
host.** That produces a number, and the number is meaningless for performance
work: it measures the emulator's translation overhead, not an ARM core's cache
behaviour, memory bandwidth, or scheduler. Publishing it would be worse than
having nothing, because it would be cited as "the ARM measurement" and would
then be used to settle CP5 — the exact failure mode the campaign's
"quarantined numbers" rule exists to prevent.

**An x86_64 Android measurement does not substitute either.** It is a real
mobile *OS* (different allocator, thermal governor, scheduler), which makes it
useful for validating the mobile code path — but it runs on this desktop CPU,
so its timings are desktop timings. It cannot answer "what does a mid-range
phone cost", which is the question CP4 was written to answer.

## What CP5 must do about it

CP5 is a written decision with "stop" as a permitted outcome. Until CP4 exists,
CP5 must record that it is being taken **without** its ARM input, and say so
explicitly rather than quietly proceeding — otherwise the campaign repeats the
N-series' documented failure of committing to an expensive phase before the
cheap measurement was taken.

## Unblocking it

Cheapest first:

1. **A physical arm64 Android device over `adb`.** Any mid-range phone.
   Everything else needed is already installed; this is the only missing piece.
2. **An arm64 CI runner** (GitHub's `ubuntu-24.04-arm` hosted runners, or a
   self-hosted Apple-silicon/Ampere box). Gives real ARM timings without a
   handset, and would make the number reproducible per-commit rather than
   one-off.
3. macOS on Apple silicon, if a runner appears for the iOS work anyway.

## What still needs building once hardware exists

The benches (`benches/benches/{perf,nodecost,identity}.rs`) are criterion host
binaries. Running them on a device needs a small harness: cross-compile with
`cargo-ndk`, push the binary and its font assets with `adb push`, run under
`adb shell`, and pull criterion's JSON back. None of it is hard, but none of it
exists yet, so "get a device" is not by itself sufficient — budget the harness
too.
