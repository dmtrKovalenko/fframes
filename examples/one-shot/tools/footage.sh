#!/usr/bin/env bash
# Rebuilds the textures in media/ from the original stock footage and photos.
# Needs macOS (Apple Vision for person and object masks), ffmpeg and a Python with
# numpy, scipy and Pillow. Only needed when changing the footage, the committed
# media/ is enough to render the video.
#
#   examples/one-shot/tools/footage.sh <work-dir> [python]
set -euo pipefail
work=$(mkdir -p "$1" && cd "$1" && pwd)
python=${2:-python3}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work/footage" "$work/photos"

clip() { curl -fsSL -o "$work/footage/$(basename "$1")" "$1"; }
clip https://videos.pexels.com/video-files/5257461/5257461-hd_1920_1080_25fps.mp4  # toy pistol
clip https://videos.pexels.com/video-files/5273821/5273821-hd_1920_1080_30fps.mp4  # girls playing tag
clip https://videos.pexels.com/video-files/10204155/10204155-hd_2048_1080_25fps.mp4  # man reaching out
clip https://videos.pexels.com/video-files/4909363/4909363-hd_1920_1080_25fps.mp4  # porridge pot

cd "$work/footage"
ffmpeg -v error -y -ss 0.75 -i 5273821-hd_1920_1080_30fps.mp4 -t 1.45 -an -c:v libx264 -crf 12 girl-run.mp4
ffmpeg -v error -y -ss 5.5 -i 10204155-hd_2048_1080_25fps.mp4 -t 4.1 -an -c:v libx264 -crf 12 reach.mp4

swiftc -O "$here/vision.swift" -o "$work/vision" 2>/dev/null
"$work/vision" mask-video 5257461-hd_1920_1080_25fps.mp4 gun-mask foreground
"$work/vision" mask-video girl-run.mp4 girl-mask instance 0.68 0.55
"$work/vision" mask-video reach.mp4 reach-mask person

photo() { curl -fsSL -o "$work/photos/$1.jpg" "$2?auto=compress&cs=tinysrgb&w=1800"; }
photo 1117543 https://images.pexels.com/photos/1117543/pexels-photo-1117543.jpeg
photo 1203819 https://images.pexels.com/photos/1203819/pexels-photo-1203819.jpeg
photo 12997264 https://images.pexels.com/photos/12997264/pexels-photo-12997264.jpeg
photo 13044706 https://images.pexels.com/photos/13044706/pexels-photo-13044706.jpeg
photo 1772123 https://images.pexels.com/photos/1772123/pexels-photo-1772123.jpeg
photo 187334 https://images.pexels.com/photos/187334/pexels-photo-187334.jpeg
photo 210927 https://images.pexels.com/photos/210927/pexels-photo-210927.jpeg
photo 30452350 https://images.pexels.com/photos/30452350/pexels-photo-30452350/free-photo-of-red-headphones-on-white-background-for-music-lovers.jpeg
photo 3394650 https://images.pexels.com/photos/3394650/pexels-photo-3394650.jpeg
photo 4523060 https://images.pexels.com/photos/4523060/pexels-photo-4523060.jpeg
photo 4678185 https://images.pexels.com/photos/4678185/pexels-photo-4678185.jpeg
photo 6461504 https://images.pexels.com/photos/6461504/pexels-photo-6461504.jpeg
photo 7138915 https://images.pexels.com/photos/7138915/pexels-photo-7138915.jpeg
photo 9227661 https://images.pexels.com/photos/9227661/pexels-photo-9227661.jpeg
"$work/vision" lift "$work/cutouts" "$work"/photos/*.jpg

"$python" -I "$here/prep.py" "$work" "$here/.."
