#!/usr/bin/env bash
# Encodes a rendered video into what the landing page plays:
#
#   scripts/landing-video.sh out.mp4 [name]
#
# landing/hls/<name>/master.m3u8  HLS with 1080p, 720p and 480p renditions, the player
#                                 picks one by bandwidth and player size and switches
#                                 while it plays
# landing/<name>.mp4              720p H.264 with faststart, played only by browsers that
#                                 can stream neither natively nor through hls.js
#
# name defaults to fframes-demo, the intro of the page; made-of-motion is the intro that
# landing-worker/ A/B tests against it. Renditions keep the aspect ratio of the input and
# its frame rate. Segments are 4 seconds long and every one starts with a keyframe, so the
# player can switch quality and seek on any segment boundary. Cloudflare serves files of at
# most 25 MiB; the segments are far below that, which is why the stream can use more bits
# than a single progressive file could.
set -euo pipefail

input="${1:?usage: scripts/landing-video.sh <rendered-video> [name]}"
name="${2:-fframes-demo}"
landing="$(cd "$(dirname "$0")/.." && pwd)/landing"
out="$landing/hls/$name"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/hls"
# -2 keeps the aspect ratio with an even width; renditions never upscale the input.
ffmpeg -hide_banner -loglevel warning -y -i "$input" \
  -filter_complex "[0:v]split=3[a][b][c];\
[a]scale=-2:'min(1080,ih)':flags=lanczos[v1080];\
[b]scale=-2:'min(720,ih)':flags=lanczos[v720];\
[c]scale=-2:'min(480,ih)':flags=lanczos[v480]" \
  -map '[v1080]' -map 0:a:0 -map '[v720]' -map 0:a:0 -map '[v480]' -map 0:a:0 \
  -c:v libx264 -preset slow -pix_fmt yuv420p -profile:v high -level:v 4.2 \
  -crf:v:0 20 -maxrate:v:0 7000k -bufsize:v:0 14000k \
  -crf:v:1 21 -maxrate:v:1 3500k -bufsize:v:1 7000k \
  -crf:v:2 22 -maxrate:v:2 1400k -bufsize:v:2 2800k \
  -force_key_frames 'expr:gte(t,n_forced*4)' -sc_threshold:v 0 -flags:v +cgop \
  -c:a aac -b:a 128k -ar:a 48000 -ac:a 2 \
  -f hls -hls_time 4 -hls_playlist_type vod -hls_segment_type fmp4 \
  -hls_flags independent_segments -hls_fmp4_init_filename init.mp4 \
  -hls_segment_filename "$tmp/hls/%v/segment-%03d.m4s" \
  -var_stream_map 'v:0,a:0,name:1080p v:1,a:1,name:720p v:2,a:2,name:480p' \
  -master_pl_name master.m3u8 "$tmp/hls/%v/index.m3u8"

# The fallback is one progressive file, so its peak bitrate is capped to fit about 22 MiB
duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$input")
fallback_kbps=$(echo "k = 22 * 1024 * 1024 * 8 / 1000 * 0.97 / $duration - 96; if (k > 2500) k = 2500; k / 1" | bc)
ffmpeg -hide_banner -loglevel error -y -i "$input" -vf "scale=-2:'min(720,ih)':flags=lanczos" \
  -c:v libx264 -preset slow -crf 24 -maxrate "${fallback_kbps}k" -bufsize "$((fallback_kbps * 2))k" \
  -profile:v high -level:v 4.1 -pix_fmt yuv420p \
  -c:a aac -b:a 96k -movflags +faststart "$tmp/$name.mp4"

limit=$((25 * 1024 * 1024))
while IFS= read -r file; do
  size=$(wc -c <"$file")
  if [ "$size" -gt "$limit" ]; then
    echo "error: $file is $((size / 1024 / 1024)) MiB, Cloudflare serves at most 25 MiB" >&2
    exit 1
  fi
done < <(find "$tmp" -type f)

rm -rf "$out"
mkdir -p "$(dirname "$out")"
mv "$tmp/hls" "$out"
mv "$tmp/$name.mp4" "$landing/"
du -sh "$out" "$landing/$name.mp4"
