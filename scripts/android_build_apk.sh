#!/usr/bin/env bash
# Build a signed, installable APK for an android cdylib example WITHOUT Gradle,
# using only the SDK build-tools (aapt2/zipalign/apksigner) + cargo-ndk (T3.1/T3.2).
#
# Usage: android_build_apk.sh <crate> <lib_name> <abi> <out.apk>
#   e.g. android_build_apk.sh hello_android hello_android x86_64 /tmp/hello.apk
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

echo "==> cross-compiling $CRATE ($ABI)"
cargo ndk -t "$ABI" -o "$STAGE/jniLibs" build -p "$CRATE" --release >/dev/null

echo "==> linking base APK (aapt2)"
"$BT/aapt2" link -o "$STAGE/base.apk" -I "$PLATFORM" \
    --manifest "$MANIFEST" --min-sdk-version 24 --target-sdk-version 34

echo "==> adding native lib"
# aapt2 ABI dir is e.g. x86_64; APK expects lib/<abi>/lib<name>.so
mkdir -p "$STAGE/lib/$ABI"
cp "$STAGE/jniLibs/$ABI/lib$LIB.so" "$STAGE/lib/$ABI/"
( cd "$STAGE" && zip -q -r base.apk "lib/$ABI/lib$LIB.so" )

echo "==> zipalign + sign"
"$BT/zipalign" -f 4 "$STAGE/base.apk" "$STAGE/aligned.apk"
"$BT/apksigner" sign \
    --ks "$HOME/.android/debug.keystore" \
    --ks-pass pass:android --ks-key-alias androiddebugkey --key-pass pass:android \
    --out "$OUT" "$STAGE/aligned.apk"

echo "==> built $OUT"
