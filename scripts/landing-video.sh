#!/usr/bin/env bash
# Encodes a rendered video into the two files the landing page plays:
#
#   scripts/landing-video.sh out.mp4 [name]
#
# landing/<name>.av1.mp4  AV1 10-bit, played by browsers with AV1 support
# landing/<name>.mp4      H.264 High@4.1, the fallback for everything else
#
# name defaults to fframes-demo, the intro of the page; made-of-motion is the intro that
# landing-worker/ A/B tests against it. Both files are SIZE (default 1280:720) with AAC
# audio and the index at the front (faststart) so playback starts before the download
# finishes. Cloudflare rejects files over 25 MiB, so the video bitrate is derived from the
# duration to land each file near TARGET_MIB (default 23).
# AV1 keeps film grain with grain synthesis: the encoder denoises, sends grain parameters
# and the decoder adds the grain back, which costs almost no bits.
set -euo pipefail

input="${1:?usage: scripts/landing-video.sh <rendered-video> [name]}"
name="${2:-fframes-demo}"
out="$(cd "$(dirname "$0")/.." && pwd)/landing"
target_mib="${TARGET_MIB:-23}"
audio_kbps=96
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$input")
# total kbit budget minus audio and ~3% container overhead, spread over the duration
video_kbps=$(echo "$target_mib * 1024 * 1024 * 8 / 1000 * 0.97 / $duration - $audio_kbps" | bc)
echo "duration ${duration}s, video ${video_kbps} kbps"

scale="scale=${SIZE:-1280:720}:flags=lanczos"

ffmpeg -hide_banner -loglevel error -y -i "$input" -vf "$scale" \
  -c:v libsvtav1 -preset 4 -b:v "${video_kbps}k" -g 240 -pix_fmt yuv420p10le \
  -svtav1-params "rc=1:tune=0:film-grain=8:film-grain-denoise=1" \
  -c:a aac -b:a "${audio_kbps}k" -movflags +faststart "$tmp/$name.av1.mp4"

for pass in 1 2; do
  target="$tmp/$name.mp4"
  [ "$pass" = 1 ] && target=/dev/null
  ffmpeg -hide_banner -loglevel error -y -i "$input" -vf "$scale" \
    -c:v libx264 -preset slow -b:v "${video_kbps}k" -pass "$pass" -passlogfile "$tmp/x264" \
    -profile:v high -level:v 4.1 -pix_fmt yuv420p -g 240 \
    -c:a aac -b:a "${audio_kbps}k" -movflags +faststart -f mp4 "$target"
done

limit=$((25 * 1024 * 1024))
for file in "$tmp"/*.mp4; do
  size=$(wc -c <"$file")
  if [ "$size" -gt "$limit" ]; then
    echo "error: $(basename "$file") is $((size / 1024 / 1024)) MiB, Cloudflare serves at most 25 MiB" >&2
    exit 1
  fi
done

mv "$tmp"/*.mp4 "$out/"
ls -lh "$out/$name".*mp4
