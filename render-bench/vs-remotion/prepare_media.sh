#!/usr/bin/env bash
# Builds the shared media for the realistic workloads (not timed). Both tools read the
# exact same files: fframes from remotion/public at runtime, Remotion via staticFile().
#
# - left30.mp4 / right30.mp4: 21 s of examples/teej-podcast/dynamic_media/{left,right}.mp4
#   starting at 30 s, re-encoded so they start on a keyframe. Same shape as the source:
#   1280x720, 24 fps, h264 Main, no B-frames, keyframe every 5 s, no audio.
# - poster.jpg: landing/poster.jpg.
# - DMSans-Medium.ttf, BebasNeue-Regular.ttf: fonts from the repo.
set -euo pipefail
cd "$(dirname "$0")"
REPO=$(cd ../.. && pwd)
PUB=remotion/public
mkdir -p "$PUB"
for side in left right; do
  [ -f "$PUB/${side}30.mp4" ] || ffmpeg -v error -y -ss 30 -i "$REPO/examples/teej-podcast/dynamic_media/$side.mp4" \
    -t 21 -an -c:v libx264 -profile:v main -crf 20 -preset medium -bf 0 -g 120 -r 24 -pix_fmt yuv420p \
    -movflags +faststart "$PUB/${side}30.mp4"
done
cp "$REPO/landing/poster.jpg" "$PUB/poster.jpg"
cp "$REPO/cargo-fframes/templates/DMSans-Medium.ttf" "$PUB/DMSans-Medium.ttf"
cp "$REPO/examples/motion-graphics/media/BebasNeue-Regular.ttf" "$PUB/BebasNeue-Regular.ttf"
ls -la "$PUB"
