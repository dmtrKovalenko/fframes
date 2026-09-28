#!/usr/bin/env bash
# Realistic workloads: podcast (1080p30, 20 s), motion (1080p60, 10 s), podcast4k (2160p30, 5 s).
# Runs every configuration ROUNDS times, interleaved (round 1 of every config of every
# workload and preset, then round 2, ...), COOLDOWN seconds idle after each run.
#
# Every run waits until $BUSY does not exist (another job on this machine renders video
# from time to time), and a run during which $BUSY appeared is discarded and redone.
# Output rows (results2.tsv):
#   workload preset config round seconds load1_before load1_after packets decoded_frames
#
# Env: ROUNDS (5), COOLDOWN (10), WORKLOADS ("podcast motion podcast4k"),
#      PRESETS ("ultrafast medium"), ONLY (regex on config names), OUT (results2.tsv),
#      REMOTION_GL ("default angle"), REMOTION_C ("8 12 16"), REMOTION_MEDIA ("offthread"), FF ("metal_ctx1 metal_ctx2 vulkan_ctx1 vulkan_ctx2")
# Prerequisites (not timed): ./prepare_media.sh, remotion bundle, cargo build --release.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(pwd)
ROUNDS=${ROUNDS:-5}
COOLDOWN=${COOLDOWN:-10}
WORKLOADS=${WORKLOADS:-"podcast motion podcast4k"}
PRESETS=${PRESETS:-"ultrafast medium"}
REMOTION_GL=${REMOTION_GL:-"default angle"}
REMOTION_C=${REMOTION_C:-"8 12 16"}
# podcast only: <OffthreadVideo> (offthread) and/or @remotion/media <Video> (mediabunny)
REMOTION_MEDIA=${REMOTION_MEDIA:-"offthread"}
FF=${FF:-"metal_ctx1 metal_ctx2 vulkan_ctx1 vulkan_ctx2"}
ONLY=${ONLY:-.}
OUT=${OUT:-$ROOT/results2.tsv}
BUSY=${BUSY:-/private/tmp/claude-501/-Users-neogoose-dev-fframes/c1da8ba8-ccab-46c9-a0d0-95fa7a45b86a/scratchpad/bench/main_busy}
BIN=$ROOT/../../target/release
export DYLD_LIBRARY_PATH=/opt/homebrew/lib
mkdir -p "$ROOT/out/r2"
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

run() { # workload preset config output command...
  local wl=$1 preset=$2 name=$3 file=$4; shift 4
  echo "$name" | grep -Eq "$ONLY" || return 0
  while true; do
    wait_idle
    local flag; flag=$(mktemp -t busyflag); rm -f "$flag"
    ( while :; do [ -e "$BUSY" ] && touch "$flag"; sleep 0.5; done ) & local watcher=$!
    local l; l=$(load1)
    local s; s=$(now)
    "$@" > "$ROOT/out/r2/$wl.$preset.$name.log" 2>&1
    local e; e=$(now)
    local l2; l2=$(load1)
    kill "$watcher" 2>/dev/null; wait "$watcher" 2>/dev/null || true
    if [ -e "$flag" ] || [ -e "$BUSY" ]; then
      rm -f "$flag"
      printf 'DISCARDED %s %s %s round %s (main_busy appeared)\n' "$wl" "$preset" "$name" "$round" | tee -a "$OUT.discarded"
      sleep "$COOLDOWN"; continue
    fi
    local t; t=$(echo "$e - $s" | bc)
    local c; c=$(count "$file")
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$wl" "$preset" "$name" "$round" "$t" "$l" "$l2" ${c} | tee -a "$OUT"
    break
  done
  sleep "$COOLDOWN"
}

R() { (cd "$ROOT/remotion" && "$@"); }

workload() { # workload preset
  local wl=$1 preset=$2 comp secs scale
  case $wl in
    podcast)   comp=Podcast;        secs=20; scale=1 ;;
    motion)    comp=MotionGraphics; secs=10; scale=1 ;;
    podcast4k) comp=Podcast4K;      secs=5;  scale=2 ;;
  esac
  local bin=podcast; [ "$wl" = motion ] && bin=motion
  local medias=offthread; [ "$wl" = motion ] || medias=$REMOTION_MEDIA
  for media in $medias; do
  for gl in $REMOTION_GL; do
    for c in $REMOTION_C; do
      local name=remotion_c${c}_gl-$gl; [ "$media" = offthread ] || name=${name}_$media
      local f=$ROOT/out/r2/$wl.$preset.$name.mp4
      local extra=(); [ "$gl" = default ] || extra=(--gl="$gl")
      [ "$media" = offthread ] || extra+=(--props="{\"media\":\"$media\"}")
      run "$wl" "$preset" "$name" "$f" R node_modules/.bin/remotion render build "$comp" "$f" \
        --codec=h264 --crf=18 --x264-preset="$preset" --concurrency="$c" --scale="$scale" --overwrite "${extra[@]}"
    done
  done
  done
  for cfg in $FF; do
    local backend=${cfg%_ctx*} ctx=${cfg#*_ctx} f=$ROOT/out/r2/$wl.$preset.fframes_$cfg.mp4
    GPU_CONTEXTS=$ctx PODCAST_SECONDS=$secs run "$wl" "$preset" "fframes_$cfg" "$f" \
      "$BIN/$bin" "$backend" "$f" "$preset" "$scale"
  done
}

for round in $(seq "${FIRST_ROUND:-1}" $(( ${FIRST_ROUND:-1} + ROUNDS - 1 ))); do
  for wl in $WORKLOADS; do
    for preset in $PRESETS; do
      workload "$wl" "$preset"
    done
  done
done
