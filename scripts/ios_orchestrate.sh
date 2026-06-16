#!/usr/bin/env bash
# `lumen run|test --platform ios_sim` backend (T3.4).
#
# iOS binaries can only be built/run on macOS with Xcode. On a Mac this drives
# the Simulator via `xcrun simctl`; on any other host it runs the headless
# verification (the platform-independent render core) and explains the rest.
set -euo pipefail
cd "$(dirname "$0")/.."
CMD="${1:-run}"

if ! command -v xcrun >/dev/null 2>&1; then
    echo "==> no Xcode/simctl (not macOS): running iOS headless verification"
    cargo test -p lumen-shell-ios -p hello_ios
    echo "==> headless OK. Full Simulator run/test requires a macOS runner;"
    echo "    see crates/lumen-shell-ios/ios/README.md."
    exit 0
fi

# --- macOS path -------------------------------------------------------------
DEVICE="${LUMEN_IOS_DEVICE:-iPhone 15}"
BUNDLE=dev.lumen.hello
TRIPLE=aarch64-apple-ios-sim

boot_sim() {
    xcrun simctl boot "$DEVICE" 2>/dev/null || true
    xcrun simctl bootstatus "$DEVICE" -b
}

build_app() {
    cargo build -p hello_ios --release --target "$TRIPLE"
    # The Xcode template links target/$TRIPLE/release/libhello_ios.a; xcodebuild
    # produces LumenHello.app. (Project generated from crates/lumen-shell-ios/ios.)
    xcodebuild -project build/ios/LumenHello.xcodeproj -scheme LumenHello \
        -sdk iphonesimulator -configuration Release -derivedDataPath build/ios/dd
}

case "$CMD" in
run)
    boot_sim
    build_app
    APP="build/ios/dd/Build/Products/Release-iphonesimulator/LumenHello.app"
    xcrun simctl install "$DEVICE" "$APP"
    xcrun simctl launch "$DEVICE" "$BUNDLE"
    # Tier-1: the host watches the app container; push a stylesheet to reload.
    xcrun simctl io "$DEVICE" screenshot /tmp/lumen_ios.png
    echo "==> launched; screenshot at /tmp/lumen_ios.png"
    ;;
test)
    boot_sim
    build_app
    cargo test -p lumen-shell-ios -p hello_ios
    echo "==> (device suite runs the M0-exit tests against the booted simulator)"
    ;;
*)
    echo "usage: ios_orchestrate.sh <run|test>" >&2
    exit 2
    ;;
esac
