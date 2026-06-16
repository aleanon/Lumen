#!/usr/bin/env bash
# T3.6: run a headless lumen-test ON the Android emulator, unmodified. Cross-
# compiles the test, pushes the test binary + golden assets to the device, and
# runs it with LUMEN_GOLDEN_DIR pointed at the pushed goldens.
#
# Usage: android_device_test.sh <crate> <test_name> <golden_src_dir>
set -euo pipefail
cd "$(dirname "$0")/.."
CRATE="${1:-hello}"; TEST="${2:-m0_exit}"; GOLDENS="${3:-examples/hello/tests/golden/cpu}"

if [[ -z "${ANDROID_HOME:-}" && -f "$HOME/android-env.sh" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/android-env.sh"
fi
export ANDROID_NDK_HOME="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:-}}"
ABI=x86_64
TRIPLE=x86_64-linux-android
DEV=/data/local/tmp/lumen-devtest

echo "==> cross-compiling $CRATE::$TEST for $TRIPLE"
cargo ndk -t "$ABI" test -p "$CRATE" --test "$TEST" --no-run 2>/dev/null
BIN="$(ls -t "target/$TRIPLE/debug/deps/${TEST}"-* | grep -v '\.d$' | head -1)"
: "${BIN:?test binary not found}"

echo "==> pushing binary + goldens to device"
adb shell "rm -rf $DEV && mkdir -p $DEV/golden"
adb push "$BIN" "$DEV/$TEST" >/dev/null
if [[ -d "$GOLDENS" ]] && compgen -G "$GOLDENS/*" >/dev/null; then
    adb push "$GOLDENS/." "$DEV/golden/" >/dev/null
fi
adb shell "chmod 755 $DEV/$TEST"

echo "==> running $TEST on device (unmodified)"
# --test-threads=1; the cdylib test runs from its own dir; point goldens at the
# pushed copy. Capture the exit code from the device shell.
adb shell "cd $DEV && LUMEN_GOLDEN_DIR=$DEV/golden ./$TEST --test-threads=1; echo DEVEXIT=\$?" \
    | tee /tmp/lumen_devtest.log
grep -q "DEVEXIT=0" /tmp/lumen_devtest.log
echo "==> $TEST passed on the emulator"
