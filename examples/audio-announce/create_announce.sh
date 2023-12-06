#!/bin/bash

set -euf -o pipefail

readonly WHISPER_CPP="$HOME/dev/whisper.cpp"
readonly WHISPER_CPP_MODEL="ggml-base.en.bin"

sox -d record.wav
ffmpeg -i record.wav -ac 1 -y -b:a 192k dynamic_media/audio.mp3

ffmpeg -i record.wav -acodec pcm_s16le -ac 1 -ar 16000 whisper-tmp.wav
${WHISPER_CPP}/main -m ${WHISPER_CPP}/models/${WHISPER_CPP_MODEL} -ml 55 -f whisper-tmp.wav -ovtt -of dynamic_media/subtitles

nvim dynamic_media/subtitles.vtt

rm -rf whisper-tmp.wav
rm -rf record.wav
cargo run --release

ffplay out.mp4

