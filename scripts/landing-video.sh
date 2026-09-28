#!/usr/bin/env bash
# Encodes a rendered video into the two files the landing page plays:
#
#   scripts/landing-video.sh out.mp4
#
# landing/fframes-demo.av1.mp4  AV1 10-bit, played by browsers with AV1 support
# landing/fframes-demo.mp4      H.264 High@4.2, the fallback for everything else
#
# Both use AAC audio and have the index at the front (faststart) so playback starts before
# the download finishes. Cloudflare rejects files over 25 MiB, so keep each one below that.
set -euo pipefail

input="${1:?usage: scripts/landing-video.sh <rendered-video>}"
out="$(cd "$(dirname "$0")/.." && pwd)/landing"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

ffmpeg -hide_banner -loglevel error -y -i "$input" \
  -c:v libsvtav1 -preset 4 -crf 30 -g 120 -pix_fmt yuv420p10le -svtav1-params tune=0 \
  -c:a aac -b:a 96k -movflags +faststart "$tmp/fframes-demo.av1.mp4"

ffmpeg -hide_banner -loglevel error -y -i "$input" \
  -c:v libx264 -preset slow -crf 21 -profile:v high -level:v 4.2 -pix_fmt yuv420p -g 120 \
  -c:a aac -b:a 96k -movflags +faststart "$tmp/fframes-demo.mp4"

limit=$((25 * 1024 * 1024))
for file in "$tmp"/*.mp4; do
  size=$(wc -c <"$file")
  if [ "$size" -gt "$limit" ]; then
    echo "error: $(basename "$file") is $((size / 1024 / 1024)) MiB, Cloudflare serves at most 25 MiB" >&2
    exit 1
  fi
done

mv "$tmp"/*.mp4 "$out/"
ls -lh "$out"/fframes-demo*.mp4
