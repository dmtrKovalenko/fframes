#!/bin/sh
set -eu
cd "$(dirname "$0")"
mkdir -p assets/hls/1080p assets/hls/720p
# Aligned four-second closed GOPs let the player switch quality and seek cleanly.
ffmpeg -hide_banner -loglevel warning -y -i ../why.mp4 \
  -filter_complex '[0:v]split=2[hi][lo];[hi]scale=1920:1080[vhi];[lo]scale=1280:720[vlo]' \
  -map '[vhi]' -map 0:a:0 -map '[vlo]' -map 0:a:0 \
  -c:v libx264 -preset fast -pix_fmt yuv420p -profile:v high -level:v 4.2 \
  -crf:v:0 22 -maxrate:v:0 6500k -bufsize:v:0 13000k \
  -crf:v:1 23 -maxrate:v:1 3000k -bufsize:v:1 6000k \
  -r:v 60 -g:v 240 -keyint_min:v 240 -sc_threshold:v 0 -flags:v +cgop \
  -c:a aac -b:a 128k -ar:a 48000 -ac:a 2 \
  -f hls -hls_time 4 -hls_playlist_type vod -hls_segment_type fmp4 \
  -hls_flags independent_segments -hls_fmp4_init_filename init.mp4 \
  -hls_segment_filename 'assets/hls/%v/segment-%03d.m4s' \
  -var_stream_map 'v:0,a:0,name:1080p v:1,a:1,name:720p' \
  -master_pl_name master.m3u8 'assets/hls/%v/index.m3u8'
