#!/usr/bin/env bash
# Build a signed, installable APK for an android cdylib example WITHOUT Gradle,
# using only the SDK build-tools (aapt2/zipalign/apksigner) + cargo-ndk (T3.1/T3.2).
#
# Usage: android_build_apk.sh <crate> <lib_name> <abi> <out.apk>
#   e.g. android_build_apk.sh hello_android hello_android x86_64 /tmp/hello.apk
#
# LUMEN_APK_LEAN=1 builds with --no-default-features, which is the profile
# CFG1/LN3 describe: no embedded pan-Unicode face, no snapshot surfaces. For
# that switch to bite, the example must declare its own feature passthrough and
# spell out its `lumen` dependency — a workspace-inherited one silently ignores
# `default-features = false` and the lean leg measures nothing. See
# examples/hello_android/Cargo.toml.
set -euo pipefail

CRATE="${1:?crate}"; LIB="${2:?lib_name}"; ABI="${3:-x86_64}"; OUT="${4:?out.apk}"
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

: "${ANDROID_HOME:?set ANDROID_HOME (source android-env.sh)}"
NDK="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:?set ANDROID_NDK_ROOT}}"
export ANDROID_NDK_HOME="$NDK"
PLATFORM="$ANDROID_HOME/platforms/android-34/android.jar"
BT="$ANDROID_HOME/build-tools/34.0.0"
MANIFEST="$ROOT/crates/lumen-shell-android/android/AndroidManifest.xml"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

FEATURES=()
[ "${LUMEN_APK_LEAN:-0}" = "1" ] && FEATURES=(--no-default-features)

echo "==> cross-compiling $CRATE ($ABI${FEATURES:+, lean})"
cargo ndk -t "$ABI" -o "$STAGE/jniLibs" build -p "$CRATE" --release "${FEATURES[@]}" >/dev/null

echo "==> linking base APK (aapt2)"
# Point the NativeActivity's lib_name at this crate's .so.
sed "s/android:value=\"hello_android\"/android:value=\"$LIB\"/" "$MANIFEST" >"$STAGE/AndroidManifest.xml"
"$BT/aapt2" link -o "$STAGE/base.apk" -I "$PLATFORM" \
    --manifest "$STAGE/AndroidManifest.xml" --min-sdk-version 24 --target-sdk-version 34

echo "==> adding native lib"
# aapt2 ABI dir is e.g. x86_64; APK expects lib/<abi>/lib<name>.so
mkdir -p "$STAGE/lib/$ABI"
cp "$STAGE/jniLibs/$ABI/lib$LIB.so" "$STAGE/lib/$ABI/"
# `-0` stores the .so uncompressed and `zipalign -p` page-aligns it, which is
# what lets Android 6+ map the library straight out of the APK instead of
# extracting a second copy at install time. Compressing it (plain `zip -r`)
# still produces a working APK, so nothing here would have failed — it just
# costs the user a duplicate of the library on disk.
( cd "$STAGE" && zip -q -0 -X base.apk "lib/$ABI/lib$LIB.so" )

echo "==> zipalign + sign"
"$BT/zipalign" -p -f 4 "$STAGE/base.apk" "$STAGE/aligned.apk"
"$BT/apksigner" sign \
    --ks "$HOME/.android/debug.keystore" \
    --ks-pass pass:android --ks-key-alias androiddebugkey --key-pass pass:android \
    --out "$OUT" "$STAGE/aligned.apk"

SZ=$(stat -c %s "$OUT")
echo "==> built $OUT — $((SZ / 1024 / 1024)).$(((SZ / 1024 / 100) % 10)) MB"
