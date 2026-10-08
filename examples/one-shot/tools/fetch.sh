#!/usr/bin/env bash
# Downloads the soundtrack and sound effects (Mixkit free licence) into
# dynamic_media/. They are not redistributed in the repository.
set -euo pipefail
cd "$(dirname "$0")/.."
out=dynamic_media
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# "State of Mind" (Mixkit #429). The cut starts 6.13 s before the drop at 72.02 s.
curl -fsSL -o "$tmp/music.mp3" https://assets.mixkit.co/music/429/429.mp3
ffmpeg -v error -y -ss 65.89 -t 28.5 -i "$tmp/music.mp3" -af "afade=t=out:st=26.0:d=2.5" -ar 48000 -ac 2 "$out/soundtrack.wav"

sfx() { # name mixkit-id [max-seconds]
  local name=$1 id=$2 len=${3:-4}
  curl -fsSL -o "$tmp/$id.wav" "https://assets.mixkit.co/active_storage/sfx/$id/$id.wav" ||
    curl -fsSL -o "$tmp/$id.wav" "https://assets.mixkit.co/active_storage/sfx/$id/$id-preview.mp3"
  # Start every effect on its transient so cue times are exact.
  ffmpeg -v error -y -i "$tmp/$id.wav" \
    -af "silenceremove=start_periods=1:start_threshold=-36dB,atrim=0:$len,areverse,afade=t=in:d=0.08,areverse" \
    -ar 48000 -ac 2 "$out/sfx-$name.wav"
}

sfx scribble 2369 1.2
sfx sweep 174 1
sfx sweep-small 166 0.8
sfx pop-hard 2364 0.6
sfx slice 2384 0.4
sfx type 2536 0.7
sfx pencil 3194 0.9
sfx sparkle 2350 2.5
sfx gun-move 1668 0.6
sfx gunshot 1662 1.6
sfx impact 2655 1.3
sfx arrow 1491 0.9
sfx run 1236 2.2
sfx pop 2358 0.5
sfx splat 263 1.4
sfx squelch 536 1.2
sfx splat-small 2361 1
sfx swirl 1493 1.6
sfx riser 2589 3.5
sfx pops 2359 1.4
sfx logo 772 1.2
ls -la "$out"
