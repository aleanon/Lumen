#!/usr/bin/env bash
# M3-exit acceptance (Android): the settings app runs on the emulator, its test
# suite passes unmodified on-device, and the agent loop (edit .lss → reload →
# screenshot) works against the emulator.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ -z "${ANDROID_HOME:-}" && -f "$HOME/android-env.sh" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/android-env.sh"
fi
export ANDROID_NDK_HOME="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:-}}"
PKG=dev.lumen.hello
EXT="/sdcard/Android/data/$PKG/files"
LSS="$EXT/lumen.lss"

red_px() { # count red pixels in a pulled screenshot via the lumen perceptual core
    python3 - "$1" <<'PY'
import struct,sys,zlib
# minimal PNG RGBA reader
def read_png(p):
    d=open(p,'rb').read(); assert d[:8]==b'\x89PNG\r\n\x1a\n'
    i=8; w=h=0; idat=b''
    while i<len(d):
        ln=struct.unpack('>I',d[i:i+4])[0]; typ=d[i+4:i+8]; data=d[i+8:i+8+ln]
        if typ==b'IHDR': w,h=struct.unpack('>II',data[:8])
        elif typ==b'IDAT': idat+=data
        i+=12+ln
    raw=zlib.decompress(idat); out=bytearray(); stride=w*4; prev=bytes(stride); o=0
    for y in range(h):
        f=raw[o]; o+=1; line=bytearray(raw[o:o+stride]); o+=stride
        for x in range(stride):
            a=line[x-4] if x>=4 else 0; b=prev[x]; c=prev[x-4] if x>=4 else 0
            if f==1: line[x]=(line[x]+a)&255
            elif f==2: line[x]=(line[x]+b)&255
            elif f==3: line[x]=(line[x]+((a+b)>>1))&255
            elif f==4:
                p=a+b-c; pa=abs(p-a); pb=abs(p-b); pc=abs(p-c)
                pr=a if pa<=pb and pa<=pc else (b if pb<=pc else c)
                line[x]=(line[x]+pr)&255
        prev=bytes(line); out+=line
    return w,h,out
w,h,px=read_png(sys.argv[1]); n=0
for j in range(0,len(px),4):
    if px[j]>150 and px[j+1]<90 and px[j+2]<90: n+=1
print(n)
PY
}

echo "==> [1/3] build + install + launch settings on the emulator"
bash scripts/android_build_apk.sh settings_android settings_android x86_64 /tmp/settings_m3.apk
adb install -r -t /tmp/settings_m3.apk
adb shell mkdir -p "$EXT"
adb shell rm -f "$LSS"                      # clean slate (no leftover stylesheet)
adb shell am force-stop "$PKG"
adb shell am start -n "$PKG/android.app.NativeActivity"
sleep 5
test -n "$(adb shell pidof "$PKG")" && echo "    settings running on device"

echo "==> [2/3] settings test suite, unmodified on-device"
bash scripts/android_device_test.sh settings agent_regression ""

echo "==> [3/3] agent loop: edit .lss → reload → screenshot"
adb exec-out screencap -p > /tmp/m3_before.png
before="$(red_px /tmp/m3_before.png)"
printf '#screen { background: #cc1111; }\n' > /tmp/m3.lss
adb push /tmp/m3.lss "$LSS" >/dev/null
sleep 3
adb exec-out screencap -p > /tmp/m3_after.png
after="$(red_px /tmp/m3_after.png)"
echo "    red pixels before=$before after=$after"
if (( after > before + 1000 )); then
    echo "==> M3-exit (Android) OK: app runs, suite green on-device, agent loop reloads"
else
    echo "agent loop did not visibly reload (before=$before after=$after)" >&2
    exit 1
fi
