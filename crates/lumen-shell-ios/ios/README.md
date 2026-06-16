# Lumen iOS host template (T3.3)

A thin UIKit host that links the Rust static lib (`libhello_ios.a`) and presents
Lumen's CPU frame via `lumen_ios_render`. Build on **macOS** only.

```sh
# cross-compile the Rust core (Apple targets need macOS + Xcode):
cargo build -p hello_ios --release --target aarch64-apple-ios-sim
# then build + run the app on a simulator:
scripts/ios_orchestrate.sh run        # boots a sim, builds, installs, launches
scripts/ios_orchestrate.sh test       # M0-exit suite on the simulator
```

Production note: swap the CoreGraphics `drawRect:` for a `CAMetalLayer` +
`MTLTexture` upload of the same bytes. Safe-area insets crop the drawable;
`UITextInput` bridges IME. Tier-2 hot patch works on the simulator (dylib swap);
physical devices are tier-3-only (code-signing forbids `dlopen` of new code).
