#!/usr/bin/env bash
# P.1 gate: the settings app on the Android emulator responds to touch and
# the soft keyboard. Boots the emulator if needed, builds + installs the
# settings APK, then drives real input via adb and asserts the screen
# changed after (a) a touch tap and (b) typed text. Taps use screen-fraction
# coordinates so the gate survives resolution changes; ANY pixel response
# counts, so it is robust to styling tweaks.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ -z "${ANDROID_HOME:-}" && -f "$HOME/android-env.sh" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/android-env.sh"
fi
: "${ANDROID_HOME:?ANDROID_HOME not set (source android-env.sh)}"
export ANDROID_NDK_HOME="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:-}}"

if ! adb devices | grep -q "emulator-.*device"; then
    avd="$(emulator -list-avds | head -1)"
    : "${avd:?no AVD available}"
    echo "==> booting emulator $avd"
    nohup emulator -avd "$avd" -no-window -no-audio -no-snapshot -no-boot-anim \
        -gpu swiftshader_indirect >/tmp/lumen-emulator.log 2>&1 &
    adb wait-for-device
    until [[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; do
        sleep 2
    done
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> building settings APK"
bash scripts/android_build_apk.sh settings_android settings_android x86_64 "$WORK/settings.apk" >/dev/null
adb install -r -t "$WORK/settings.apk" >/dev/null
adb shell am force-stop dev.lumen.hello
adb shell am start -n dev.lumen.hello/android.app.NativeActivity >/dev/null
sleep 5

read -r W H < <(adb shell wm size | sed -n 's/.*: \([0-9]*\)x\([0-9]*\)/\1 \2/p')
tap_frac() { adb shell input tap "$(python3 -c "print(int($W*$1))")" "$(python3 -c "print(int($H*$2))")"; }

shot() { adb exec-out screencap -p >"$WORK/$1"; }
differs() {
    python3 - "$WORK/$1" "$WORK/$2" <<'PY'
import sys, zlib
a, b = open(sys.argv[1], 'rb').read(), open(sys.argv[2], 'rb').read()
sys.exit(0 if a != b else 1)
PY
}

shot base.png
echo "==> touch: toggle the Notifications switch"
tap_frac 0.045 0.183
sleep 1.5
shot touched.png
differs base.png touched.png || { echo "FAIL: touch produced no screen change"; exit 1; }
echo "touch OK"

echo "==> text: focus the About username field + type"
tap_frac 0.46 0.155   # About tab
sleep 1
tap_frac 0.145 0.213  # username field (focus → soft keyboard)
sleep 1.5
shot focused.png
adb shell input text gate
sleep 1.5
shot typed.png
differs focused.png typed.png || { echo "FAIL: typing produced no screen change"; exit 1; }
echo "text OK"

echo "==> back button must not kill the app"
adb shell input keyevent 4
sleep 1.5
adb shell pidof dev.lumen.hello >/dev/null || { echo "FAIL: back press killed the app"; exit 1; }
echo "back OK"

echo "android input gate OK"
