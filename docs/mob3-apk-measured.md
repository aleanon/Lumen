# MOB3 — a real APK, built, installed, and driven. The block was inherited.

*2026-08-08.*

MOB3 sat on the blocked list for this whole campaign with the reason *"no APK or
IPA has ever been built in this repo"* (`docs/backlog.md:70`). That was wrong,
and the repository said so in two places I never opened:

* **`scripts/android_build_apk.sh` already existed** — 45 lines driving
  `cargo-ndk` + `aapt2` + `zipalign` + `apksigner`, no Gradle, written for
  T3.1/T3.2.
* **The reachability study said the opposite**: *"the toolchain has already
  been proven to work once; this is re-enabling and hardening, not building
  from scratch"* (`path-to-a-plus/03-resource-path.md:727`).

The NDK, SDK build-tools, `cargo-apk`, `cargo-ndk`, all three Android rustup
targets and an API-34 AVD were all installed. Worse, `adb install` failed with
`INSTALL_FAILED_UPDATE_INCOMPATIBLE` — **a `dev.lumen.hello` from an earlier
session was still installed on the emulator.** The evidence against the block
was sitting on the device the block was about.

## What was actually verified

Not "it compiles" — the whole path, on an API-34 emulator:

| step | result |
|---|---|
| cross-compile `aarch64-linux-android` | 23.4 MB stripped ARM64 ELF |
| package + sign | `apksigner verify` passes |
| install | `Success` |
| launch | `lumen android shell starting`, displayed **+163 ms** |
| render | text and a rounded filled button, correct |
| input | three taps → label reads `Hello Lumen — 3` |

The last row is the one that matters: it exercises touch → hit-test → handler →
signal write → rebuild → repaint on a real Android runtime. A screenshot alone
would only have proven the first frame.

`dumpsys gfxinfo` reports **0 frames rendered**, and that is *expected*, not a
failure — a `NativeActivity` drawing to its own surface never touches HWUI's
counters. Worth writing down, because it looks exactly like a broken app.

## Size, measured on both profiles

| profile | arm64-v8a | x86_64 |
|---|---:|---:|
| default | **22.8 MB** | 22.4 MB |
| `--no-default-features` | **7.0 MB** | 8.5 MB |

7.0 MB confirms the estimate in `crates/lumen/Cargo.toml`'s own feature comment
(*"hello: 22 MB → ~7 MB"*) to the stated precision. The lean APK was installed
and driven too: identical behaviour for Latin text.

**The lean leg silently measured nothing at first.** `--no-default-features`
applied to `hello_android`, which had no features, while its
`lumen = { workspace = true }` kept every default — so both builds came out at
22.8 MB and the "lean" figure was the full one wearing a label. This is the
workspace-inheritance trap for the eleventh time this campaign:
**`default-features = false` is ignored on a workspace-inherited dependency.**
The example now spells its `lumen` and `lumen-shell-android` deps out by path
and forwards its own `pan-unicode`/`snapshot` features.

That failure mode is the dangerous one: a lean gate that inherits its deps
reports a number, passes, and measures the wrong build.

## What this does not close

**CP4 is still blocked, and for a different reason than MOB3 was.** The emulator
is x86_64 under KVM; the arm64 APK is built but never executed. An arm64 AVD
would be QEMU-emulated, and a timing taken there would measure QEMU. MOB3 asked
"does it build and run" — answered. CP4 asks "how fast on ARM" — still needs
hardware.

**iOS remains genuinely blocked**: no macOS, so no `xcodebuild`, no simulator,
no IPA. Unlike Android, nothing here contradicts that.

## The lesson, stated plainly

The two Android items were filed under one heading and had nothing in common.
MOB3 was blocked by an unchecked assumption; CP4 is blocked by physics. Bundling
them meant the false block inherited the true one's credibility, and neither got
re-examined for the length of a campaign. **A blocked list needs its reasons
re-read, not just its items re-counted** — this one was contradicted by a script
in the same repo, a sentence in its own study, and an app already installed on
the test device.
