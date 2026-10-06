"""Prepare the supplied reference audio and a selected native-window recording.

No preview is opened and no promo video is rendered by this command.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def probe(path):
    return json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-show_streams', '-show_format', '-of', 'json', str(path)
    ]))


def audio_hash(path):
    return subprocess.check_output([
        'ffmpeg', '-v', 'error', '-i', str(path), '-map', '0:a:0', '-c:a', 'copy',
        '-f', 'hash', '-hash', 'sha256', '-'
    ], text=True).strip().split('=', 1)[1]


def ffmpeg(*args):
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', *map(str, args)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('recording', type=Path)
    parser.add_argument('--start', type=float, default=24.1)
    parser.add_argument('--end', type=float, default=26.5)
    parser.add_argument('--crop', default='2824:1652:68:54', help='width:height:x:y of the settled window')
    args = parser.parse_args()
    video = next(s for s in probe(args.reference)['streams'] if s['codec_type'] == 'video')
    if video['r_frame_rate'] != '30000/1001' or int(video['nb_frames']) != 932:
        parser.error('This edit expects the 932-frame, 30000/1001 fps reference.')
    capture_duration = float(probe(args.recording)['format']['duration'])
    if args.start < 0 or args.end > capture_duration or args.end - args.start < 71 / 30:
        parser.error('Select at least 71/30 seconds entirely within the recording.')
    runtime = ROOT / 'dynamic_media'
    runtime.mkdir(exist_ok=True)
    audio = runtime / 'reference-audio.m4a'
    ffmpeg('-i', args.reference, '-map', '0:a:0', '-c:a', 'copy', '-y', audio)
    original_hash = audio_hash(args.reference)
    if audio_hash(audio) != original_hash:
        raise RuntimeError('The extracted AAC packets differ from the original.')
    # MediaDirectory currently does not load M4A. A float PCM decode preserves the
    # original stereo signal for native preview; retain untouched AAC for export.
    ffmpeg('-i', audio, '-map', '0:a:0', '-c:a', 'pcm_f32le', '-y', runtime / 'reference-audio.wav')
    clip = runtime / 'native-scrubbing.mp4'
    ffmpeg('-ss', args.start, '-i', args.recording, '-t', args.end - args.start,
           '-vf', f'crop={args.crop},scale=1920:-2,fps=30', '-an',
           '-c:v', 'libx264', '-preset', 'fast', '-crf', '14', '-pix_fmt', 'yuv420p',
           '-movflags', '+faststart', '-y', clip)
    ffmpeg('-i', clip, '-frames:v', '1', '-q:v', '2', '-y', ROOT / 'media/native-preview-poster.jpg')
    evidence = {'reference': str(args.reference.resolve()), 'recording': str(args.recording.resolve()),
                'in': args.start, 'out': args.end, 'crop': args.crop, 'speed': 1.0,
                'aac_sha256': original_hash, 'frame_count': 932, 'source_fps': '30000/1001'}
    (runtime / 'preparation.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(json.dumps(evidence, indent=2))


if __name__ == '__main__':
    main()
