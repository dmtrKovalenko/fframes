#!/usr/bin/env bash
# TextFx workload (3,334 large text nodes per frame with seeded animated effects, 1080p30,
# 300 frames). Same measurement rules as run2.sh: every configuration ROUNDS times,
# interleaved (round 1 of every config and preset, then round 2, ...), COOLDOWN seconds idle
# after each run, wall time of the whole command. Every run waits until $BUSY does not exist,
# and a run during which $BUSY appeared is discarded and redone.
# Output rows (results_textfx.tsv):
#   preset config round seconds load1_before load1_after packets decoded_frames
#
# Env: ROUNDS (5), COOLDOWN (10), PRESETS ("ultrafast medium"), ONLY (regex on config names),
#      OUT (results_textfx.tsv), REMOTION ("c8 c12 c16 c8_angle c16_angle"),
#      FF ("metal_ctx1 metal_ctx2 vulkan_ctx1 vulkan_ctx2 cpu_ctx1")
# Prerequisites (not timed): remotion bundle, one warm-up render, cargo build --release.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(pwd)
ROUNDS=${ROUNDS:-5}
COOLDOWN=${COOLDOWN:-10}
PRESETS=${PRESETS:-"ultrafast medium"}
REMOTION=${REMOTION:-"c8 c12 c16 c8_angle c16_angle"}
FF=${FF:-"metal_ctx1 metal_ctx2 vulkan_ctx1 vulkan_ctx2 cpu_ctx1"}
ONLY=${ONLY:-.}
OUT=${OUT:-$ROOT/results_textfx.tsv}
BUSY=${BUSY:-/private/tmp/claude-501/-Users-neogoose-dev-fframes/c1da8ba8-ccab-46c9-a0d0-95fa7a45b86a/scratchpad/bench/main_busy}
BIN=$ROOT/../../target/release/textfx
export DYLD_LIBRARY_PATH=/opt/homebrew/lib
mkdir -p "$ROOT/out/textfx"
[ "${APPEND:-0}" = 1 ] || : > "$OUT"

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
load1() { sysctl -n vm.loadavg | awk '{print $2}'; }
wait_idle() {
  while [ -e "$BUSY" ]; do echo "  main_busy present, waiting..." >&2; sleep 15; done
}
count() { # packets, decoded frames
  local p d
  p=$(ffprobe -v error -select_streams v -show_entries packet=pts -of csv=p=0 "$1" | wc -l | tr -d ' ')
  d=$(ffprobe -v error -select_streams v -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$1" | tr -d ',')
  echo "$p $d"
}

run() { # preset config output command...
  local preset=$1 name=$2 file=$3; shift 3
  echo "$name" | grep -Eq "$ONLY" || return 0
  while true; do
    wait_idle
    local flag; flag=$(mktemp -t busyflag); rm -f "$flag"
    ( while :; do [ -e "$BUSY" ] && touch "$flag"; sleep 0.5; done ) & local watcher=$!
    local l; l=$(load1)
    local s; s=$(now)
    "$@" > "$ROOT/out/textfx/$preset.$name.log" 2>&1
    local e; e=$(now)
    local l2; l2=$(load1)
    kill "$watcher" 2>/dev/null; wait "$watcher" 2>/dev/null || true
    if [ -e "$flag" ] || [ -e "$BUSY" ]; then
      rm -f "$flag"
      printf 'DISCARDED %s %s round %s (main_busy appeared)\n' "$preset" "$name" "$round" | tee -a "$OUT.discarded"
      sleep "$COOLDOWN"; continue
    fi
    local t; t=$(echo "$e - $s" | bc)
    local c; c=$(count "$file")
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$preset" "$name" "$round" "$t" "$l" "$l2" "${c// /$'\t'}" | tee -a "$OUT"
    break
  done
  sleep "$COOLDOWN"
}

R() { (cd "$ROOT/remotion" && "$@"); }

configs() { # preset
  local preset=$1
  for cfg in $REMOTION; do
    local c=${cfg%%_*} gl=default; [ "$cfg" = "${cfg%_angle}" ] || gl=angle
    local name=remotion_${c}_gl-$gl f=$ROOT/out/textfx/$preset.remotion_${c}_gl-$gl.mp4
    local extra=(); [ "$gl" = default ] || extra=(--gl="$gl")
    run "$preset" "$name" "$f" R node_modules/.bin/remotion render build TextFx "$f" \
      --codec=h264 --crf=18 --x264-preset="$preset" --concurrency="${c#c}" --overwrite "${extra[@]}"
  done
  for cfg in $FF; do
    local backend=${cfg%_ctx*} ctx=${cfg#*_ctx} name=fframes_$cfg
    [ "$backend" = cpu ] && name=fframes_cpu
    local f=$ROOT/out/textfx/$preset.$name.mp4
    GPU_CONTEXTS=$ctx run "$preset" "$name" "$f" "$BIN" "$backend" "$f" "$preset"
  done
}

for round in $(seq "${FIRST_ROUND:-1}" $(( ${FIRST_ROUND:-1} + ROUNDS - 1 ))); do
  for preset in $PRESETS; do configs "$preset"; done
done
