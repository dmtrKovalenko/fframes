"""Synthesize the original sound assets. Rust places and mixes them on the timeline."""

import array
import math
from pathlib import Path
import random
import sys
import wave

RATE = 48_000
TAU = 2 * math.pi


def sine(freq, t):
    return math.sin(TAU * freq * t)


def noise_filter(seed, low, high):
    rng = random.Random(seed)
    fast, slow = 0.0, 0.0
    high_coefficient = 1 - math.exp(-TAU * high / RATE)
    low_coefficient = 1 - math.exp(-TAU * low / RATE)

    def sample():
        nonlocal fast, slow
        value = rng.uniform(-1, 1)
        fast += (value - fast) * high_coefficient
        slow += (value - slow) * low_coefficient
        return fast - slow

    return sample


crackle = noise_filter(629, 110, 2100)
tick_noise = noise_filter(99, 760, 8900)
swipe_noise = noise_filter(430, 270, 2800)
riser_noise = noise_filter(431, 690, 3800)
snap_noise = noise_filter(988, 850, 12000)
sweep_noise = noise_filter(5432, 270, 3600)
ash_noise = noise_filter(9184, 170, 2900)
paper_noise = noise_filter(9186, 380, 3100)


def room(t):
    motion = 0.72 + 0.20 * sine(0.22, t) + 0.08 * sine(0.64, t)
    pedal = 0.064 * sine(55, t) + 0.048 * sine(82.407, t)
    air = 0.040 * sine(110.1, t) + 0.029 * sine(164.8, t) + 0.017 * sine(220.31, t)
    return (pedal + air + 0.075 * crackle()) * motion


def tick(t):
    edge = tick_noise() * math.exp(-185 * t) * 0.34
    bell = sine(990 - 1550 * t, t) * math.exp(-150 * t) * 0.18
    tap = sine(472 - 810 * t, t) * math.exp(-133 * t) * 0.11
    return min(1, 1400 * t) * (edge + bell + tap)


def swipe(t):
    envelope = math.sin(math.pi * t / 0.31) ** 2
    return envelope * (
        0.26 * swipe_noise()
        + 0.095 * sine(134 - 155 * t, t)
        + 0.034 * sine(205 - 160 * t, t)
    )


def riser(t):
    remaining = 1 - t / 1.38
    envelope = min(1, t / 0.31, remaining * 21) * (t / 1.38) ** 0.92
    tremolo = 0.74 + 0.26 * sine(4.5 + 14 * t, t)
    charge = 0.22 * sine(43 + 15 * t + 18 * t * t, t) + 0.14 * sine(95 + 57 * t, t)
    coil = 0.079 * sine(241 + 171 * t, t) + 0.08 * riser_noise()
    return (charge * tremolo + coil) * envelope


def snap(t):
    # Two short broadband contact impulses lead into a separated room echo.
    age = t - 0.007
    if age < 0:
        return 0.0
    attack = min(1.0, 3300 * age)
    slap = (0.94 * snap_noise() + 0.31 * sine(2160 - 22300 * age, age)) * math.exp(-285 * age)
    knuckle = (0.26 * sine(973 - 11000 * age, age) + 0.25 * sine(426, age)) * math.exp(-139 * age)
    echo = 0.0
    for delay, gain in ((0.032, 0.095), (0.071, 0.047), (0.124, 0.030)):
        tail = age - delay
        if tail > 0:
            echo += gain * (sine(960 - 2800 * tail, tail) + snap_noise()) * math.exp(-61 * tail)
    return attack * (slap + knuckle) + echo


def shock(t):
    pitch = TAU * (45 * t + 9 * (1 - math.exp(-4 * t)))
    mid = TAU * (83 * t + 7 * (1 - math.exp(-5 * t)))
    boom = 0.49 * math.sin(pitch) * math.exp(-4.4 * t)
    chest = 0.24 * math.sin(mid) * math.exp(-7.4 * t)
    rushing_air = 0.29 * sweep_noise() * math.exp(-6.4 * t)
    return min(1.0, 90 * t) * (boom + chest + rushing_air)


def dust(t):
    swell = min(1.0, 5 * t) * math.exp(-1.48 * t)
    scatter = 0.69 + 0.18 * sine(8.6, t) + 0.13 * sine(17.12, t)
    return swell * (0.25 * ash_noise() * scatter + 0.027 * sine(117 - 38 * t, t))


def card(t):
    pop = 0.24 * sine(176 - 245 * t, t) * math.exp(-37 * t)
    paper = 0.30 * paper_noise() * math.exp(-52 * t)
    overtone = 0.085 * sine(465 - 270 * t, t) * math.exp(-51 * t)
    return min(1.0, 350 * t) * (pop + paper + overtone)


def write(directory, name, duration, voice):
    frames = round(duration * RATE)
    samples = array.array(
        "h",
        (round(32760 * max(-1, min(1, voice(i / RATE)))) for i in range(frames)),
    )
    if sys.byteorder != "little":
        samples.byteswap()
    with wave.open(str(directory / (name + ".wav")), "wb") as out:
        out.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
        out.writeframes(samples.tobytes())


def main():
    destination = Path(sys.argv[1])
    destination.mkdir(parents=True, exist_ok=True)
    for name, duration, voice in (
        ("room", 19.0, room),
        ("tick", 0.08, tick),
        ("swipe", 0.31, swipe),
        ("riser", 1.38, riser),
        ("snap", 0.22, snap),
        ("shock", 1.46, shock),
        ("dust", 2.12, dust),
        ("card", 0.17, card),
    ):
        write(destination, name, duration, voice)


if __name__ == "__main__":
    main()
