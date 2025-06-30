#!/bin/bash
set -euf -o pipefail

readonly WHISPER_CPP="$HOME/dev/whisper.cpp"
readonly WHISPER_CPP_MODEL="ggml-base.en.bin"

sox -c 1 -r 44100 -d dynamic_media/audio.wav 

ffmpeg -i dynamic_media/audio.wav -acodec pcm_s16le -ac 1 -ar 16000 whisper-tmp.wav
${WHISPER_CPP}/build/bin/whisper-cli -m ${WHISPER_CPP}/models/${WHISPER_CPP_MODEL} -ml 55 -f whisper-tmp.wav -ovtt -of dynamic_media/subtitles

nvim dynamic_media/subtitles.vtt

rm whisper-tmp.wav
ffmpeg -y -i dynamic_media/audio.wav dynamic_media/audio.mp3 
rm dynamic_media/audio.wav

cargo run --release
ffplay out.mp4

