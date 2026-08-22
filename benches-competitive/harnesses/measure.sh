#!/usr/bin/env bash
# Distributable size + idle RSS, one method for every framework in the
# comparison so the rows are actually comparable.
#
# size: the executable, plus the sum of the UNIQUE shared objects `ldd` resolves
#       for it, excluding the C runtime that every binary on the system already
#       has (libc, libm, libdl, libpthread, librt, ld-linux). A statically
#       linked Rust binary therefore reports its own size and nothing else,
#       while GTK/Qt report what they actually drag in — which is the honest
#       comparison, and the reason BENCH3 wrote GTK as "14 KB + 30.8 MB".
# rss:  median of 3 launches, VmRSS from /proc/<pid>/status, 4 s after launch.
set -uo pipefail
export LC_ALL=C   # printf %f is locale-sensitive; a comma decimal breaks it

size_of() {
  local exe="$1" total=0 self
  self=$(stat -c%s "$exe")
  local libs
  libs=$(ldd "$exe" 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i ~ /^\//) print $i}' | sort -u \
         | grep -vE '/(libc|libm|libdl|libpthread|librt|ld-linux[^/]*)\.so' || true)
  local n=0
  while read -r l; do
    [ -z "$l" ] && continue
    total=$((total + $(stat -Lc%s "$l" 2>/dev/null || echo 0)))
    n=$((n+1))
  done <<< "$libs"
  printf "%.1f MB exe + %.1f MB in %d shared libs" \
    "$(echo "scale=4; $self/1048576" | bc)" "$(echo "scale=4; $total/1048576" | bc)" "$n"
}

rss_of() {
  local exe="$1"; shift
  local vals=()
  for _ in 1 2 3; do
    "$exe" "$@" >/dev/null 2>&1 &
    local pid=$!
    sleep 4
    local v
    v=$(awk '/^VmRSS:/{print $2}' /proc/$pid/status 2>/dev/null || echo 0)
    vals+=("$v")
    kill $pid 2>/dev/null; wait $pid 2>/dev/null
  done
  printf '%s\n' "${vals[@]}" | sort -n | awk 'NR==2{printf "%.1f MB", $1/1024}'
}

case "${1:-}" in
  size) size_of "$2" ;;
  rss)  shift; rss_of "$@" ;;
  *) echo "usage: measure.sh size <exe> | measure.sh rss <exe> [args]" >&2; exit 2 ;;
esac
