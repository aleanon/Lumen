#!/usr/bin/env bash
# Run the CI gate locally, before pushing.
#
# WHY: CI is the only thing that runs `lean`, `gpu`, `executors`, `fonts`,
# `perf` and `live-window`, and `just check` covered none of them. The push of
# 2026-08-12 went red on FOUR jobs at once (clippy, lean, cold-start, gpu) —
# every one of which is reproducible on this box in minutes. This script is the
# mirror, so red is discovered before the push rather than after it.
#
# Each leg names the `.github/workflows/ci.yml` job it stands in for. When a leg
# fails it prints that job name and the exact command to re-run, because the
# point of a local gate is to shorten the loop, not to hide it behind a wrapper.
#
# TARGET DIRECTORY: this deliberately shares the normal `target/`. Giving the
# gate its own would be more faithful (CI starts cold) but this box is at 85%
# disk with a 120 GB `target/` already, and a second tree would not fit.
#
# RUSTFLAGS: CI sets `RUSTFLAGS: -D warnings` globally. Setting it here for the
# workspace legs would fork every dependency fingerprint and double the build
# cache for no coverage: `cargo clippy --workspace --all-targets -- -D warnings`
# already denies rustc warnings across exactly the same target set. The `lean`
# leg DOES export it, because that job runs `cargo check` with no clippy step
# and its whole documented reason for existing is a `-D warnings` break.
#
# Usage:
#   scripts/ci_local.sh              # fast tier (the default the hook uses)
#   scripts/ci_local.sh --full       # every leg this machine can run
#   scripts/ci_local.sh --list       # show the legs and their tiers
#   scripts/ci_local.sh --only gpu --only lean
#   scripts/ci_local.sh --skip perf
#
# Exit status is 0 only if every leg that ran passed.

# NOT `-e`: legs are run individually and their failures collected, so one red
# leg does not hide the state of the rest.
set -uo pipefail
cd "$(dirname "$0")/.."

# The leg registry uses associative arrays (bash 4+). macOS still ships bash
# 3.2 as /bin/bash, where `declare -A` fails with a misleading syntax error.
if [ "${BASH_VERSINFO[0]:-0}" -lt 4 ]; then
  echo "this script needs bash 4+ (found ${BASH_VERSION:-unknown})." >&2
  echo "on macOS: brew install bash" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Leg registry. tier: fast | full. job: the ci.yml job it mirrors.
# ---------------------------------------------------------------------------
LEGS=(fmt clippy deny test doc lean executors gpu fonts perf live fuzz)

declare -A TIER=(
  [fmt]=fast [clippy]=fast [deny]=fast [test]=fast [doc]=fast
  [lean]=fast [executors]=fast
  [gpu]=full [fonts]=full [perf]=full [live]=full [fuzz]=full
)
declare -A JOB=(
  [fmt]="lint"      [clippy]="lint"       [deny]="lint"
  [test]="test"     [doc]="test"          [lean]="lean"
  [executors]="executors"                 [gpu]="gpu"
  [fonts]="fonts"   [perf]="perf"         [live]="live-window"
  [fuzz]="fuzz(nightly)"
)
declare -A WHAT=(
  [fmt]="cargo fmt --all --check"
  [clippy]="cargo clippy --workspace --all-targets -- -D warnings"
  [deny]="cargo deny check"
  [test]="cargo build --workspace --all-targets && cargo test --workspace"
  [doc]="cargo test --workspace --doc"
  [lean]="per-crate --no-default-features check (RUSTFLAGS=-D warnings)"
  [executors]="cargo test -p lumen-exec --features tokio,smol + runtime containment"
  [gpu]="cargo test -p lumen-render -p lumen-widgets --features wgpu"
  [fonts]="scripts/subset_fonts.sh verify + pan-unicode goldens"
  [perf]="perf_gate.sh + cold_start_gate.sh + size_gate.sh"
  [live]="scripts/live_window_gate.sh"
  [fuzz]="replay the committed corpus through all four libFuzzer targets"
)

# ---------------------------------------------------------------------------
# Legs.
#
# A leg returns 0 (pass), 1 (fail), or 77 (skipped — a prerequisite this
# machine does not have). 77 is reported separately and never counts as green:
# a gate that silently no-ops when a tool is missing reports success while
# proving nothing, which is the failure mode `LUMEN_REQUIRE_GPU` exists for.
# ---------------------------------------------------------------------------
SKIP_CODE=77

leg_fmt()    { cargo fmt --all --check; }
leg_clippy() { cargo clippy --workspace --all-targets -- -D warnings; }

leg_deny() {
  command -v cargo-deny >/dev/null || {
    echo "cargo-deny not installed: cargo install cargo-deny --locked"; return $SKIP_CODE; }
  local out rc
  out=$(cargo deny check 2>&1); rc=$?
  echo "$out"
  # A stale local cargo-deny cannot parse newer advisory entries (CVSS 4.0
  # arrived in RUSTSEC-2024-0445). That is a broken TOOL, not a finding about
  # this repo's dependencies — CI runs cargo-deny-action@v2, which ships a
  # current binary. Reporting it as FAIL would be a false red, and a gate that
  # cries wolf gets bypassed; reporting it as SKIP keeps it visible and unGREEN.
  if [ $rc -ne 0 ] && grep -q "failed to load advisory database" <<<"$out"; then
    echo
    echo "^ the LOCAL advisory database failed to parse. This says nothing about"
    echo "  this repo. Installed: $(cargo deny --version). Fix with:"
    echo "      cargo install cargo-deny --locked"
    return $SKIP_CODE
  fi
  return $rc
}

leg_test() {
  cargo build --workspace --all-targets || return 1
  cargo test --workspace
}

leg_doc() { cargo test --workspace --doc; }

leg_lean() {
  # Subshell, deliberately: `export` in a function body persists for the rest of
  # the script, so without this the RUSTFLAGS below would leak into every later
  # leg and silently rebuild them under different flags — the exact fingerprint
  # fork the file header explains we are avoiding.
  ( _leg_lean_inner )
}

_leg_lean_inner() {
  # See the RUSTFLAGS note in the file header for why this leg — and only this
  # leg — sets it.
  export RUSTFLAGS="-D warnings"
  local crate
  # Tier 1: lib + tests.
  for crate in lumen-core lumen-style lumen-text lumen-render \
               lumen-layout lumen-shell-core; do
    printf '  %-18s ' "$crate"
    cargo check -q -p "$crate" --no-default-features --all-targets || return 1
    echo "OK (with tests)"
  done
  # Tier 2: lib only — see .github/workflows/ci.yml for why.
  for crate in lumen-app lumen-widgets lumen-shell lumen; do
    printf '  %-18s ' "$crate"
    cargo check -q -p "$crate" --no-default-features || return 1
    echo "OK (lib)"
  done
}

leg_executors() {
  cargo test -p lumen-exec --features tokio,smol || return 1
  # The default graph must contain neither runtime (ADR-003 amendment, 07 EX).
  if cargo tree -p lumen -e normal | grep -qiE "^[^a-z]*(tokio|smol) "; then
    echo "a default lumen build pulled an async runtime — the containment that"
    echo "justifies the ADR-003 amendment has regressed (see 07, EX)."
    cargo tree -p lumen -e normal | grep -iE "tokio|smol"
    return 1
  fi
}

leg_gpu() {
  # Fail rather than self-skip once we know a driver exists: LUMEN_REQUIRE_GPU
  # makes a missing adapter an error instead of a silent green.
  command -v vulkaninfo >/dev/null || {
    echo "no vulkan loader: sudo apt install mesa-vulkan-drivers vulkan-tools"
    return $SKIP_CODE; }
  LUMEN_REQUIRE_GPU=1 cargo test -p lumen-render -p lumen-widgets --features wgpu
}

leg_fonts() {
  python3 -c 'import fontTools' 2>/dev/null || {
    echo "fontTools missing: pip install fonttools==4.63.0"
    echo "(the version is pinned — subsetter output is version-dependent)"
    return $SKIP_CODE; }
  ./scripts/subset_fonts.sh verify || return 1
  cargo test -p lumen-text --features pan-unicode
}

leg_perf() {
  ./scripts/perf_gate.sh || return 1
  ./scripts/cold_start_gate.sh || return 1
  ./scripts/size_gate.sh
}

leg_live() {
  [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] || {
    echo "no display; this leg opens a real window (CI uses Xvfb :99)"
    return $SKIP_CODE; }
  scripts/live_window_gate.sh
}

leg_fuzz() {
  # NOT the nightly job. That job generates NEW random input for 300 s per
  # target and is what finds new defects; it cannot be reproduced on demand and
  # a green run here says nothing about tonight's run.
  #
  # What this leg does is a regression replay: run the committed corpus
  # (~1000 inputs) through each target once, with a 5 s per-input ceiling so a
  # performance regression on a KNOWN input is caught. That is deterministic
  # and takes seconds.
  command -v cargo-fuzz >/dev/null || {
    echo "cargo-fuzz not installed: cargo install cargo-fuzz --locked"; return $SKIP_CODE; }
  rustup toolchain list | grep -q '^nightly' || {
    echo "no nightly toolchain: rustup toolchain install nightly"; return $SKIP_CODE; }
  local t
  for t in lss_parse selector agent_json decode; do
    echo "  -> $t ($(ls "fuzz/corpus/$t" | wc -l) inputs)"
    cargo +nightly fuzz run "$t" -- -runs=0 -timeout=5 || return 1
  done
  if compgen -G "fuzz/artifacts/*/*" >/dev/null; then
    echo "fuzz artifacts present:"; ls -R fuzz/artifacts
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Argument parsing.
# ---------------------------------------------------------------------------
WANT_TIER=fast
ONLY=()
SKIP=()

while [ $# -gt 0 ]; do
  case "$1" in
    --fast) WANT_TIER=fast ;;
    --full) WANT_TIER=full ;;
    --only) ONLY+=("$2"); shift ;;
    --skip) SKIP+=("$2"); shift ;;
    --list)
      printf '%-11s %-6s %-12s %s\n' LEG TIER "CI JOB" WHAT
      for l in "${LEGS[@]}"; do
        printf '%-11s %-6s %-12s %s\n' "$l" "${TIER[$l]}" "${JOB[$l]}" "${WHAT[$l]}"
      done
      exit 0 ;;
    -h|--help) sed -n '2,40p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

# Disk preflight. `cargo build --workspace --all-targets` writes ~25 GB into
# target/debug on this box (51 example crates × every target kind), and running
# out mid-build does not fail cleanly: rustc reports "couldn't create a temp
# dir" or "failed to build archive", which reads like a code error and sent this
# gate's own author hunting a phantom flaky test twice. Refuse up front instead.
MIN_FREE_GB=${LUMEN_CI_MIN_FREE_GB:-30}
preflight_disk() {
  local free_gb
  free_gb=$(df -BG --output=avail . | tail -1 | tr -dc '0-9')
  [ -n "$free_gb" ] || return 0          # unknown: do not block on a parse miss
  if [ "$free_gb" -lt "$MIN_FREE_GB" ]; then
    echo "REFUSING TO START: ${free_gb} GB free, need ~${MIN_FREE_GB} GB." >&2
    echo >&2
    echo "A full-workspace --all-targets build writes ~25 GB to target/debug." >&2
    echo "Running out mid-build fails as a confusing rustc error, not a disk one." >&2
    echo >&2
    echo "Reclaim (largest first, all pure cache):" >&2
    du -sh target/debug/* 2>/dev/null | sort -rh | head -4 | sed 's/^/  /' >&2
    echo >&2
    echo "  rm -rf target/debug/examples     # rebuilt on demand" >&2
    echo "  cargo clean                      # all of it" >&2
    echo >&2
    echo "Override once with: LUMEN_CI_MIN_FREE_GB=0 scripts/ci_local.sh" >&2
    return 1
  fi
}

selected() {
  local l=$1 s
  if [ ${#ONLY[@]} -gt 0 ]; then
    for s in "${ONLY[@]}"; do [ "$s" = "$l" ] && return 0; done
    return 1
  fi
  for s in "${SKIP[@]}"; do [ "$s" = "$l" ] && return 1; done
  [ "$WANT_TIER" = full ] || [ "${TIER[$l]}" = fast ]
}

# ---------------------------------------------------------------------------
# Run.
# ---------------------------------------------------------------------------
PASSED=(); FAILED=(); SKIPPED=()
declare -A SECS
START=$SECONDS

preflight_disk || exit 1

echo "=============================================================="
if [ ${#ONLY[@]} -gt 0 ]; then
  echo " Lumen CI gate — selected legs: ${ONLY[*]}"
else
  echo " Lumen CI gate — ${WANT_TIER} tier"
fi
echo "=============================================================="

for leg in "${LEGS[@]}"; do
  selected "$leg" || continue
  echo
  echo "--- ${leg}  [ci job: ${JOB[$leg]}]  ${WHAT[$leg]}"
  t0=$SECONDS
  "leg_$leg"
  rc=$?
  SECS[$leg]=$((SECONDS - t0))
  case $rc in
    0)          PASSED+=("$leg") ;;
    $SKIP_CODE) SKIPPED+=("$leg") ;;
    *)          FAILED+=("$leg") ;;
  esac
done

# ---------------------------------------------------------------------------
# Report. Everything this gate could NOT vouch for is printed explicitly —
# a green run that quietly omitted half the matrix is worse than no gate.
# ---------------------------------------------------------------------------
echo
echo "=============================================================="
for l in "${PASSED[@]:-}";  do [ -n "$l" ] && printf ' PASS  %-11s %4ds\n' "$l" "${SECS[$l]}"; done
for l in "${SKIPPED[@]:-}"; do [ -n "$l" ] && printf ' SKIP  %-11s %4ds  (missing prerequisite)\n' "$l" "${SECS[$l]}"; done
for l in "${FAILED[@]:-}";  do [ -n "$l" ] && printf ' FAIL  %-11s %4ds  -> ci job "%s"\n' "$l" "${SECS[$l]}" "${JOB[$l]}"; done
printf ' total %ds\n' $((SECONDS - START))

echo
echo "NOT covered by this gate, whatever it reported above:"
echo "  - build + test on windows-latest and macos-latest (CI matrix; this box is Linux)"
# Derived, not hardcoded: a footer that names the wrong legs is the same lie it
# exists to prevent.
NOTRUN=()
for l in "${LEGS[@]}"; do selected "$l" || NOTRUN+=("$l"); done
if [ ${#NOTRUN[@]} -gt 0 ]; then
  echo "  - legs that did not run: ${NOTRUN[*]}"
fi
if [ ${#SKIPPED[@]} -gt 0 ] && [ -n "${SKIPPED[0]:-}" ]; then
  echo "  - legs that could not run here: ${SKIPPED[*]}"
fi
echo "  - the NIGHTLY fuzz job, which generates new input each run. It is not"
echo "    triggered by push and no pre-push gate can predict it."
echo "=============================================================="

if [ ${#FAILED[@]} -gt 0 ]; then
  echo
  echo "reproduce:"
  for l in "${FAILED[@]}"; do echo "  scripts/ci_local.sh --only $l"; done
  exit 1
fi
exit 0
