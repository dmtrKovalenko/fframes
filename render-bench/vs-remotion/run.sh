#!/usr/bin/env bash
# Runs every configuration ROUNDS times, interleaved (round 1 of all configs, then round 2, ...)
# so that background load drifts hit all configs alike. Wall time of each command is written
# to results.tsv as: config <tab> round <tab> seconds <tab> 1-min load average before the run.
#
# Prerequisites (not timed): see README.md (npm install, remotion bundle, cargo build --release).
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(pwd)
ROUNDS=${ROUNDS:-3}
# Seconds to idle after every run so load and heat from one run do not leak into the next.
COOLDOWN=${COOLDOWN:-20}
FF=$ROOT/../../target/release/vs-remotion-bench
export DYLD_LIBRARY_PATH=/opt/homebrew/lib
OUT=$ROOT/results.tsv
mkdir -p "$ROOT/out"
[ "${APPEND:-0}" = 1 ] || : > "$OUT"

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
load1() { sysctl -n vm.loadavg | awk '{print $2}'; }

run() { # name, command...
  local name=$1; shift
  local l; l=$(load1)
  local s; s=$(now)
  "$@" > "$ROOT/out/$name.log" 2>&1
  local e; e=$(now)
  local t; t=$(echo "$e - $s" | bc)
  printf '%s\t%s\t%s\t%s\n' "$name" "$round" "$t" "$l" | tee -a "$OUT"
  sleep "${COOLDOWN:-0}"
}

R() { (cd "$ROOT/remotion" && "$@"); }

configs() {
  # Remotion, exactly as documented: bundles + renders, default concurrency (8x on 16 cores).
  run remotion_default       R npx remotion render src/index.ts TextGrid ../out/remotion_default.mp4 --codec=h264 --overwrite
  # Remotion, pre-bundled (bundling excluded) and with explicit concurrency.
  for c in 8 12 16; do
    run remotion_prebundled_c$c R node_modules/.bin/remotion render build TextGrid ../out/remotion_c$c.mp4 --codec=h264 --overwrite --concurrency=$c
  done
  # fframes, same encoder settings (libx264 crf 18, preset medium, yuv420p).
  GPU_CONTEXTS=1 run fframes_metal_ctx1 "$FF" metal  "$ROOT/out/fframes_metal_ctx1.mp4" medium
  GPU_CONTEXTS=2 run fframes_metal_ctx2 "$FF" metal  "$ROOT/out/fframes_metal_ctx2.mp4" medium
  GPU_CONTEXTS=2 run fframes_vulkan_ctx2 "$FF" vulkan "$ROOT/out/fframes_vulkan_ctx2.mp4" medium
  run fframes_cpu                        "$FF" cpu    "$ROOT/out/fframes_cpu.mp4" medium
  # Secondary: x264 preset ultrafast on both sides, to see the renderers without the encoder
  # being the bottleneck.
  run remotion_prebundled_c8_ultrafast R node_modules/.bin/remotion render build TextGrid ../out/remotion_uf.mp4 --codec=h264 --overwrite --concurrency=8 --x264-preset=ultrafast
  GPU_CONTEXTS=2 run fframes_metal_ctx2_ultrafast "$FF" metal "$ROOT/out/fframes_metal_uf.mp4" ultrafast
  run fframes_cpu_ultrafast "$FF" cpu "$ROOT/out/fframes_cpu_uf.mp4" ultrafast
}

for round in $(seq "${FIRST_ROUND:-1}" $(( ${FIRST_ROUND:-1} + ROUNDS - 1 ))); do configs; done
