#!/usr/bin/env bash
# Startup and memory baseline.
#
# Startup is measured to the point the window is mapped by the compositor,
# which is what "feels fast" actually means — not process exit of main().
#
# Usage: scripts/bench-startup.sh [runs] [path-to-binary]
set -euo pipefail

runs="${1:-5}"
binary="${2:-${CARGO_TARGET_DIR:-target}/debug/tela}"

if [ ! -x "$binary" ]; then
  echo "no binary at $binary" >&2
  exit 1
fi

if ! command -v hyprctl >/dev/null 2>&1; then
  echo "hyprctl not found; this script measures window-map time under Hyprland" >&2
  exit 1
fi

echo "binary:  $binary"
echo "size:    $(du -h "$binary" | cut -f1)"
echo

total=0
for run in $(seq 1 "$runs"); do
  start=$(date +%s%N)

  "$binary" >/dev/null 2>&1 &
  pid=$!

  # Poll until the compositor reports the window. 2 ms is well under the
  # measurement's own noise floor.
  while ! hyprctl clients -j 2>/dev/null | grep -q '"class": *"tela"'; do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "process died before mapping a window" >&2
      exit 1
    fi
    sleep 0.002
  done

  end=$(date +%s%N)
  ms=$(( (end - start) / 1000000 ))
  total=$(( total + ms ))

  # Settle before sampling memory so lazy allocations have happened.
  sleep 1
  rss_kb=$(awk '/VmRSS/ {print $2}' "/proc/$pid/status" 2>/dev/null || echo 0)
  procs=$(pgrep -c -P "$pid" 2>/dev/null || echo 0)

  printf 'run %d: %4d ms to window   RSS %5d MB   child processes %d\n' \
    "$run" "$ms" "$(( rss_kb / 1024 ))" "$procs"

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  sleep 0.5
done

echo
printf 'mean: %d ms over %d runs\n' "$(( total / runs ))" "$runs"
