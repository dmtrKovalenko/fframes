#!/usr/bin/env bash
# Short side-by-side sample clips from the benchmark outputs (not timed):
# first 6 s, 1280x720, h264 crf 20, 30 fps, no audio, plus middle-frame PNGs.
#   ./make_clips.sh <clips dir> <workload>=<remotion.mp4>,<fframes.mp4> ...
set -euo pipefail
dir=$1; shift
mkdir -p "$dir"
for spec in "$@"; do
  wl=${spec%%=*}; files=${spec#*=}; r=${files%%,*}; f=${files#*,}
  for tool in remotion fframes; do
    src=$r; [ $tool = fframes ] && src=$f
    ffmpeg -v error -y -i "$src" -t 6 -vf "fps=30,scale=1280:720:flags=lanczos,format=yuv420p" -an \
      -c:v libx264 -crf 20 -preset slow -movflags +faststart "$dir/${wl}_$tool.mp4"
    # middle frame of the clip (3 s) and of the full-length output, full resolution
    ffmpeg -v error -y -i "$dir/${wl}_$tool.mp4" -vf "select=eq(n\,90)" -frames:v 1 "$dir/${wl}_$tool.png"
    n=$(ffprobe -v error -select_streams v -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$src" | tr -d ,)
    ffmpeg -v error -y -i "$src" -vf "select=eq(n\,$((n / 2)))" -frames:v 1 "$dir/${wl}_${tool}_fullres_mid.png"
  done
done
ls -la "$dir"
