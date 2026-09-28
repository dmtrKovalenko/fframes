"""Deterministic synthesized placeholder music; no downloaded or licensed recordings."""
import math
import pathlib
import struct
import wave

RATE = 48000
SECONDS = 24
ROOT = pathlib.Path(__file__).resolve().parents[1]
# Four six-second scenes, with a soft pluck every half second.
CHORDS = [(130.81, 164.81, 196.00), (110.00, 130.81, 164.81),
          (87.31, 110.00, 130.81), (98.00, 123.47, 146.83)]
output = bytearray()
for i in range(RATE * SECONDS):
    t = i / RATE
    scene = min(int(t / 6), 3)
    chord = CHORDS[scene]
    beat = t % 0.5
    note = chord[int(t * 2) % 3] * 4
    env = (1 - math.exp(-beat * 100)) * math.exp(-beat * 7)
    pluck = (math.sin(2 * math.pi * note * t) + 0.25 * math.sin(4 * math.pi * note * t)) * env
    local = t % 6
    pad_env = min(local / 0.2, 1) * min((6 - local) / 0.25, 1)
    pad = sum(math.sin(2 * math.pi * f * t) for f in chord) / 3 * pad_env
    tick = math.sin(2 * math.pi * 1200 * beat) * math.exp(-beat * 100) * (1 - math.exp(-beat * 2000))
    fade = min(t / 0.02, 1) * min((SECONDS - t) / 0.6, 1)
    sample = (0.20 * pluck + 0.14 * pad + 0.05 * tick) * fade
    output.extend(struct.pack('<h', int(max(-1, min(1, sample)) * 32767)))
with wave.open(str(ROOT / 'media' / 'pulse.wav'), 'wb') as audio:
    audio.setnchannels(1)
    audio.setsampwidth(2)
    audio.setframerate(RATE)
    audio.writeframes(output)
