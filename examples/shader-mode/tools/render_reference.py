"""Explicit, opt-in final export with the reference clock and untouched audio."""
import argparse
import json
from pathlib import Path
import subprocess
from prepare_media import ROOT, probe, audio_hash, ffmpeg


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True, help='Built shader-mode executable')
    parser.add_argument('-o', '--output', type=Path, default=ROOT / 'output/shader-mode.mp4')
    args = parser.parse_args()
    audio = ROOT / 'dynamic_media/reference-audio.m4a'
    recording = ROOT / 'dynamic_media/native-scrubbing.mp4'
    if not audio.is_file() or not recording.is_file():
        parser.error('Run tools/prepare_media.py with the reference and native recording first.')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    intermediate = args.output.with_name(args.output.stem + '.30fps.mp4')
    subprocess.run([str(args.binary.resolve()), 'render', '-o', str(intermediate)], check=True)
    # Preserve the exact source-frame sequence; only its timestamps change. The
    # source's AAC stream bypasses the renderer's decoder/mixer/encoder entirely.
    ffmpeg('-i', intermediate, '-i', audio, '-map', '0:v:0', '-map', '1:a:0',
           '-vf', 'settb=1/30000,setpts=N*1001', '-r', '30000/1001', '-fps_mode', 'cfr',
           '-c:v', 'libx264', '-crf', '16', '-preset', 'medium', '-pix_fmt', 'yuv420p',
           '-c:a', 'copy', '-video_track_timescale', '30000', '-movflags', '+faststart',
           '-y', args.output)
    video = next(s for s in probe(args.output)['streams'] if s['codec_type'] == 'video')
    if int(video['nb_frames']) != 932 or video['r_frame_rate'] != '30000/1001':
        raise RuntimeError('Export frame count or clock differs from the reference.')
    if audio_hash(args.output) != audio_hash(audio):
        raise RuntimeError('Export changed the source audio packets.')
    evidence = {'frames': 932, 'fps': video['r_frame_rate'], 'audio_packets_identical': True,
                'output': str(args.output.resolve())}
    args.output.with_suffix('.verification.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(json.dumps(evidence, indent=2))


if __name__ == '__main__':
    main()
