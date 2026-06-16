#!/usr/bin/env bash
# `lumen run|test --platform android` backend (T3.2): provision the emulator,
# build the APK, install, wire the dev socket, launch, and stream logs.
set -euo pipefail
cd "$(dirname "$0")/.."
CMD="${1:-run}"

# Source the local Android env if the SDK isn't already on PATH.
if [[ -z "${ANDROID_HOME:-}" && -f "$HOME/android-env.sh" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/android-env.sh"
fi
: "${ANDROID_HOME:?ANDROID_HOME not set (install the SDK or source android-env.sh)}"
export ANDROID_NDK_HOME="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:-}}"
DEV_PORT="${LUMEN_DEV_PORT:-8765}"
PKG=dev.lumen.hello

ensure_emulator() {
    if adb devices | grep -q "emulator-.*device"; then
        return
    fi
    local avd
    avd="$(emulator -list-avds | head -1)"
    : "${avd:?no AVD available — create one (see android-toolchain memory)}"
    echo "==> booting emulator $avd"
    nohup emulator -avd "$avd" -no-window -no-audio -no-snapshot -no-boot-anim \
        -gpu swiftshader_indirect -netdelay none -netspeed full >/tmp/lumen-emulator.log 2>&1 &
    adb wait-for-device
    # Wait for full boot.
    until [[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; do
        sleep 2
    done
}

case "$CMD" in
run)
    ensure_emulator
    bash scripts/android_build_apk.sh hello_android hello_android x86_64 /tmp/lumen_run.apk
    adb install -r -t /tmp/lumen_run.apk
    # Dev socket: reverse the host dev-server port onto the device (tier-1).
    adb reverse "tcp:$DEV_PORT" "tcp:$DEV_PORT" || true
    adb shell am force-stop "$PKG" || true
    adb shell am start -n "$PKG/android.app.NativeActivity"
    echo "==> $PKG launched; streaming logs (Ctrl-C to stop)"
    exec adb logcat -v brief lumen_shell_android:I '*:S'
    ;;
test)
    ensure_emulator
    # The M0-exit suite, cross-compiled and run unmodified on the emulator (T3.6).
    bash scripts/android_device_test.sh hello m0_exit examples/hello/tests/golden/cpu
    # Plus the device shell + tier-1 reload checks.
    cargo test -p lumen-shell-android --test device_golden --test tier1_reload -- --ignored
    ;;
*)
    echo "usage: android_orchestrate.sh <run|test>" >&2
    exit 2
    ;;
esac
