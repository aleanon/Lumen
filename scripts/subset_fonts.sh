#!/usr/bin/env bash
# LN1: regenerate (or verify) the two DERIVED font faces, reproducibly.
#
# Both subsets shipped as untracked build outputs: someone ran pyftsubset once,
# committed the .ttf, and the recipe lived only in that person's shell history.
# That makes them unauditable — you cannot tell what a face contains, cannot
# re-cut it when the upstream font updates, and cannot check that the committed
# bytes came from the source they claim.
#
# Both are now byte-for-byte reproducible. The ranges below were RECOVERED FROM
# THE COMMITTED ARTIFACTS (their cmap coverage), not guessed, then verified to
# regenerate each file with an identical SHA-256:
#
#   GoNotoKurrent-Latin.ttf   354 748 B  e8980d71e4e1bbe70646cbb09248d5de62e3085a2e4c3a1fcaca4e95de94d25e
#   DejaVuSans-Symbols.ttf    171 732 B  338d318e4e06fb763aa7b5550aa42d2d130a0025de9f8ee6447eac31f92c014a
#
# Modes:
#   verify    (default) regenerate into a temp dir and compare against the
#             committed files. Fails on any difference.
#   generate  overwrite the committed files.
#
# Requires fontTools. It FAILS when absent rather than skipping: a gate that
# silently no-ops reports green while proving nothing.
set -euo pipefail
cd "$(dirname "$0")/.."

MODE="${1:-verify}"

if ! command -v pyftsubset >/dev/null; then
    echo "FAIL: pyftsubset not found. Install with 'pip install fonttools'," >&2
    echo "      or in a venv: python3 -m venv .venv && .venv/bin/pip install fonttools" >&2
    echo "      then run with PATH=\$PWD/.venv/bin:\$PATH" >&2
    exit 1
fi

FONTS=crates/lumen-text/fonts

# The pan-Unicode face is the SOURCE of the Latin subset and is itself
# committed, so that half is reproducible from the repo alone.
NOTO_SRC="$FONTS/GoNotoKurrent-Regular.ttf"

# DejaVu is NOT in the repo — only its subset is, plus its licence. Point
# DEJAVU_SRC at a DejaVuSans.ttf. The committed subset was cut from
# fontRevision 2.37, which is what Debian/Ubuntu's fonts-dejavu-core ships.
DEJAVU_SRC="${DEJAVU_SRC:-/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf}"

# Recovered coverage. Worth noting the code comment in lumen-text/src/lib.rs
# described the DejaVu subset as "U+2000-2BFF symbol blocks": the real coverage
# is 45 discrete ranges reaching U+FFFD, so that description could never have
# re-cut the file. This is why the recipe is recorded as data rather than prose.
LATIN_RANGES="U+0000,U+000D,U+001D,U+0020-007E,U+00A0-0377,U+037A-037F,U+0384-038A,U+038C,U+038E-03A1,U+03A3-03FF,U+1E00-1EFF,U+2000-2064,U+2066-2071,U+2074-208E,U+2090-209C,U+20A0-20C0,U+2100-215F,U+2183-2184,U+2189,U+2212,U+2219,U+22EE,U+25CC,U+262C,U+2638,U+2670-2671,U+FB00-FB06,U+FFFD"
DEJAVU_RANGES="U+2000-2064,U+206A-2071,U+2074-208E,U+2090-209C,U+2100-2109,U+210B-2149,U+214B,U+214E,U+2150-2185,U+2189,U+2190-2311,U+2318-2319,U+231C-2321,U+2324-2328,U+232B-232C,U+2373-2375,U+237A,U+237D,U+2387,U+2394,U+239B-23AE,U+23CE-23CF,U+23E3,U+23E5,U+23E8,U+2500-257F,U+25A0-269C,U+269E-26B8,U+26C0-26C3,U+26E2,U+2701-2704,U+2706-2709,U+270C-2727,U+2729-274B,U+274D,U+274F-2752,U+2756,U+2758-275E,U+2761-2794,U+2798-27AF,U+27B1-27BE,U+2B00-2B1A,U+2B1F-2B24,U+2B53-2B54,U+FFFD"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

status=0

# name, source, ranges, committed path.
# `--layout-features='*'` is load-bearing: the default drops layout features the
# shaper needs, and both outputs stop matching (Latin 354 748 -> 317 900).
check_face() {
    name="$1"; src="$2"; ranges="$3"; committed="$4"
    if [ ! -f "$src" ]; then
        echo "FAIL: $name source not found: $src" >&2
        if [ "$name" = "DejaVuSans-Symbols" ]; then
            echo "      set DEJAVU_SRC=/path/to/DejaVuSans.ttf (fontRevision 2.37)" >&2
        fi
        return 1
    fi
    pyftsubset "$src" --unicodes="$ranges" --layout-features='*' \
        --output-file="$TMP/$name.ttf"
    if [ "$MODE" = "generate" ]; then
        cp "$TMP/$name.ttf" "$committed"
        echo "regenerated $committed ($(stat -c%s "$committed") bytes)"
        return 0
    fi
    if cmp -s "$TMP/$name.ttf" "$committed"; then
        echo "OK   $name reproduces byte-for-byte ($(stat -c%s "$committed") bytes)"
        return 0
    fi
    echo "FAIL $name does NOT reproduce" >&2
    echo "     committed   $(stat -c%s "$committed") bytes" >&2
    echo "     regenerated $(stat -c%s "$TMP/$name.ttf") bytes" >&2
    echo "     A different fontTools version also causes this; the committed" >&2
    echo "     faces were cut with fontTools 4.63.0." >&2
    return 1
}

check_face GoNotoKurrent-Latin "$NOTO_SRC" "$LATIN_RANGES" \
    "$FONTS/GoNotoKurrent-Latin.ttf" || status=1
check_face DejaVuSans-Symbols "$DEJAVU_SRC" "$DEJAVU_RANGES" \
    "$FONTS/DejaVuSans-Symbols.ttf" || status=1

if [ "$status" -eq 0 ]; then
    echo "font subsets OK"
fi
exit "$status"
