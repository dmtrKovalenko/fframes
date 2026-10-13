//! Synthesizes the sound of FFRAMES, ASSEMBLED into `dynamic_media/`. The music follows the
//! story beats, and the servo sound follows the joint speeds of the animation.
//!
//! `cargo run --release -p arm-example --bin arm-soundtrack`

use std::f32::consts::{PI, TAU};
use std::path::Path;

use arm_example::Show;
use fframes_3d::math::{hash, smoothstep};

/// Samples per second.
const RATE: f32 = 48_000.0;

// ------------------------------------------------------------------ Building blocks

/// White noise from a fixed seed (xorshift).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// An RBJ biquad filter. Call `set` for each sample to sweep the filter.
#[derive(Default, Clone)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

/// Low pass, high pass or band pass.
#[derive(Clone, Copy)]
enum Kind {
    Low,
    High,
    Band,
}

impl Biquad {
    fn new(kind: Kind, freq: f32, q: f32) -> Self {
        let mut f = Self::default();
        f.set(kind, freq, q);
        f
    }

    fn set(&mut self, kind: Kind, freq: f32, q: f32) {
        let w = TAU * freq.clamp(10.0, RATE * 0.45) / RATE;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match kind {
            Kind::Low => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0),
            Kind::High => (
                f32::midpoint(1.0, cos),
                -(1.0 + cos),
                f32::midpoint(1.0, cos),
            ),
            Kind::Band => (alpha, 0.0, -alpha),
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2.0 * cos / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A band limited sawtooth (polyBLEP).
#[derive(Default, Clone)]
struct Saw {
    phase: f32,
}

impl Saw {
    fn next(&mut self, freq: f32) -> f32 {
        let dt = (freq / RATE).min(0.5);
        let mut y = 2.0 * self.phase - 1.0;
        y -= blep(self.phase, dt);
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

/// The polyBLEP correction near the jump of the sawtooth.
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// A sine oscillator.
#[derive(Default, Clone)]
struct Sine {
    phase: f32,
}

impl Sine {
    fn next(&mut self, freq: f32) -> f32 {
        let y = (self.phase * TAU).sin();
        self.phase = (self.phase + freq / RATE).fract();
        y
    }
}

/// The frequency of a MIDI note in Hz.
fn note(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Exponential decay from `t = 0`, silence before.
fn decay(t: f32, rate: f32) -> f32 {
    if t < 0.0 { 0.0 } else { (-t * rate).exp() }
}

/// A linear attack of `attack` seconds, then exponential decay.
fn attack_decay(t: f32, attack: f32, rate: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < attack {
        t / attack
    } else {
        (-(t - attack) * rate).exp()
    }
}

/// A stereo buffer at [`RATE`].
#[derive(Clone)]
struct Track {
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Track {
    fn new(seconds: f32) -> Self {
        let n = (seconds * RATE) as usize;
        Self {
            l: vec![0.0; n],
            r: vec![0.0; n],
        }
    }

    fn len(&self) -> usize {
        self.l.len()
    }

    /// Adds a mono signal at `start` seconds. `pan` goes from -1 (left) to 1 (right) with
    /// equal power.
    fn add(&mut self, start: f32, signal: &[f32], gain: f32, pan: f32) {
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
        let (gl, gr) = (angle.cos() * gain, angle.sin() * gain);
        let offset = (start * RATE).round() as isize;
        for (i, s) in signal.iter().enumerate() {
            let j = offset + i as isize;
            if j >= 0 && (j as usize) < self.l.len() {
                self.l[j as usize] += s * gl;
                self.r[j as usize] += s * gr;
            }
        }
    }

    /// Adds `other`, scaled by `gain`.
    fn mix(&mut self, other: &Track, gain: f32) {
        for i in 0..self.len().min(other.len()) {
            self.l[i] += other.l[i] * gain;
            self.r[i] += other.r[i] * gain;
        }
    }

    /// Scales the track so that its peak is at `peak_db`.
    fn normalize(&mut self, peak_db: f32) {
        let peak = self
            .l
            .iter()
            .chain(&self.r)
            .fold(0.0f32, |m, s| m.max(s.abs()));
        if peak > 0.0 {
            let gain = 10f32.powf(peak_db / 20.0) / peak;
            self.l
                .iter_mut()
                .chain(self.r.iter_mut())
                .for_each(|s| *s *= gain);
        }
    }

    /// Writes a 16 bit stereo WAV file.
    fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut bytes = Vec::with_capacity(44 + self.len() * 4);
        let data = (self.len() * 4) as u32;
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(RATE as u32).to_le_bytes());
        bytes.extend_from_slice(&(RATE as u32 * 4).to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data.to_le_bytes());
        for i in 0..self.len() {
            for s in [self.l[i], self.r[i]] {
                bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
            }
        }
        std::fs::write(path, bytes)
    }
}

/// A stereo reverb in the style of Freeverb.
fn reverb(input: &Track, room: f32, damp: f32, wet: f32) -> Track {
    let combs = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    let allpasses = [556, 441, 341, 225];
    let mut out = input.clone();
    for (channel, spread) in [(0usize, 0usize), (1, 23)] {
        let src = if channel == 0 { &input.l } else { &input.r };
        let mut acc = vec![0.0f32; src.len()];
        for &size in &combs {
            let size = (size + spread) * 48 / 44;
            let mut buf = vec![0.0f32; size];
            let mut idx = 0;
            let mut store = 0.0;
            for (i, x) in src.iter().enumerate() {
                let y = buf[idx];
                store = y * (1.0 - damp) + store * damp;
                buf[idx] = x * 0.015 + store * room;
                idx = (idx + 1) % size;
                acc[i] += y;
            }
        }
        for &size in &allpasses {
            let size = (size + spread) * 48 / 44;
            let mut buf = vec![0.0f32; size];
            let mut idx = 0;
            for a in &mut acc {
                let b = buf[idx];
                let y = -*a + b;
                buf[idx] = *a + b * 0.5;
                idx = (idx + 1) % size;
                *a = y;
            }
        }
        let dst = if channel == 0 { &mut out.l } else { &mut out.r };
        for (d, a) in dst.iter_mut().zip(&acc) {
            *d = *d * (1.0 - wet * 0.3) + a * wet;
        }
    }
    out
}

/// Samples `f(t)` for `seconds`.
fn signal(seconds: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..(seconds * RATE) as usize)
        .map(|i| f(i as f32 / RATE))
        .collect()
}

/// A centered track from a mono signal, normalized to `peak_db`.
fn mono(signal: &[f32], peak_db: f32) -> Track {
    let mut t = Track::new(signal.len() as f32 / RATE);
    t.add(0.0, signal, 1.0, 0.0);
    t.normalize(peak_db);
    t
}

// ------------------------------------------------------------------ Instruments

/// Modal percussion. Each partial is (frequency ratio, amplitude, decay per second).
fn modal(freq: f32, partials: &[(f32, f32, f32)], length: f32, mallet: f32, seed: u32) -> Vec<f32> {
    let mut oscs: Vec<Sine> = partials.iter().map(|_| Sine::default()).collect();
    let mut noise = Noise(seed);
    let mut low = Biquad::new(Kind::Low, 3000.0, 0.7);
    signal(length, |t| {
        let tone: f32 = oscs
            .iter_mut()
            .zip(partials)
            .map(|(o, (r, a, d))| o.next(freq * r) * a * decay(t, *d))
            .sum();
        let hit = low.run(noise.next()) * decay(t, 400.0) * mallet;
        (tone + hit) * smoothstep(0.0, 0.002, t)
    })
}

fn marimba(midi: f32, velocity: f32) -> Vec<f32> {
    let v = velocity;
    modal(
        note(midi),
        &[
            (1.0, 1.0, 5.0),
            (3.93, 0.28 * v, 14.0),
            (9.2, 0.08 * v, 30.0),
        ],
        1.0,
        0.5 * v,
        midi as u32,
    )
    .into_iter()
    .map(|s| s * v)
    .collect()
}

fn glock(midi: f32, velocity: f32) -> Vec<f32> {
    modal(
        note(midi),
        &[
            (1.0, 1.0, 1.4),
            (2.756, 0.45, 2.8),
            (5.404, 0.2, 4.5),
            (8.933, 0.08, 7.0),
        ],
        2.2,
        0.3,
        midi as u32 + 7,
    )
    .into_iter()
    .map(|s| s * velocity)
    .collect()
}

fn music_box(midi: f32) -> Vec<f32> {
    modal(
        note(midi),
        &[(1.0, 1.0, 1.8), (3.0, 0.15, 5.0), (4.2, 0.08, 7.0)],
        2.0,
        0.1,
        midi as u32 + 13,
    )
}

/// Karplus-Strong plucked string.
fn pluck(midi: f32, length: f32, damping: f32, seed: u32) -> Vec<f32> {
    let freq = note(midi);
    let n = (RATE / freq).max(2.0) as usize;
    let mut noise = Noise(seed);
    let mut low = Biquad::new(Kind::Low, freq * 6.0, 0.7);
    let mut buf: Vec<f32> = (0..n).map(|_| low.run(noise.next())).collect();
    let mut i = 0;
    signal(length, |t| {
        let j = (i + 1) % n;
        let y = buf[i];
        buf[i] = f32::midpoint(buf[i], buf[j]) * damping;
        i = j;
        y * decay(t, 2.0)
    })
}

/// A short pizzicato: a pluck that stops quickly.
fn pizz(midi: f32, velocity: f32, seed: u32) -> Vec<f32> {
    let s = pluck(midi, 0.45, 0.995, seed);
    s.into_iter()
        .enumerate()
        .map(|(i, x)| x * velocity * decay(i as f32 / RATE, 7.0))
        .collect()
}

/// A funny double reed: FM synthesis with a small vibrato.
fn bassoon(midi: f32, length: f32, slide: f32) -> Vec<f32> {
    let mut carrier = 0.0f32;
    let mut modulator = 0.0f32;
    let mut low = Biquad::new(Kind::Low, 1400.0, 0.8);
    signal(length, |t| {
        let f = note(midi + slide * (t / length))
            * (1.0 + 0.008 * (t * 31.0).sin() * smoothstep(0.1, 0.3, t));
        modulator = (modulator + f * 2.0 / RATE).fract();
        let index = 1.8 * (0.6 + 0.4 * decay(t, 3.0));
        carrier = (carrier + f / RATE).fract();
        let y = (carrier * TAU + index * (modulator * TAU).sin()).sin();
        low.run(y) * attack_decay(t, 0.03, 0.8) * (1.0 - smoothstep(length - 0.08, length, t))
    })
}

fn soft_kick() -> Vec<f32> {
    let mut osc = Sine::default();
    signal(0.4, |t| {
        (osc.next(48.0 + 70.0 * decay(t, 35.0)) * 1.4).tanh() * decay(t, 9.0)
    })
}

fn brush(seed: u32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut band = Biquad::new(Kind::Band, 3500.0, 0.6);
    signal(0.25, |t| {
        band.run(noise.next()) * attack_decay(t, 0.012, 14.0) * 0.8
    })
}

fn shaker(seed: u32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut high = Biquad::new(Kind::High, 6500.0, 0.7);
    signal(0.08, |t| {
        high.run(noise.next()) * attack_decay(t, 0.008, 60.0) * 0.5
    })
}

fn pad(notes: &[f32], length: f32, attack: f32) -> Vec<f32> {
    let mut saws: Vec<[Saw; 3]> = notes.iter().map(|_| Default::default()).collect();
    let mut low = Biquad::new(Kind::Low, 1400.0, 0.6);
    signal(length, |t| {
        let raw: f32 = notes
            .iter()
            .zip(saws.iter_mut())
            .map(|(n, s)| {
                let f = note(*n);
                s[0].next(f * 0.997) + s[1].next(f) + s[2].next(f * 1.003)
            })
            .sum();
        low.run(raw)
            * smoothstep(0.0, attack, t)
            * (1.0 - smoothstep(length - 0.4, length, t))
            * 0.06
    })
}

fn crickets(length: f32, seed: u32) -> Vec<f32> {
    let mut a = Sine::default();
    let mut noise = Noise(seed);
    signal(length, |t| {
        // Each 0.7 s cycle has a chirp of three pulses. Some cycles are silent.
        let cycle = (t / 0.7).floor();
        let within = t - cycle * 0.7;
        let on = hash(cycle as u32 + seed) > 0.25;
        let pulse = (within * 32.0).fract() < 0.5 && within < 0.09;
        let env = if on && pulse { 1.0 } else { 0.0 };
        (a.next(4600.0 + 60.0 * noise.next()) * env) * 0.4
    })
}

// ------------------------------------------------------------------ The score

/// One beat at 120 BPM, in seconds.
const BEAT: f32 = 0.5;

/// Slows `track` down to a stop in `length` seconds from `at`, as a tape machine without
/// power does. Everything after the stop is silent.
fn tape_stop(track: &mut Track, at: f32, length: f32) {
    let start = (at * RATE) as usize;
    let n = (length * RATE) as usize;
    let source = track.clone();
    for i in 0..track.len().saturating_sub(start) {
        let (l, r) = if i < n {
            let s = i as f32 / n as f32;
            // The read position moves with the speed (1 - s)^2.
            let pos = start as f32 + n as f32 / 3.0 * (1.0 - (1.0 - s).powi(3));
            let k = pos as usize;
            let f = pos - k as f32;
            let get = |v: &[f32]| {
                v.get(k).copied().unwrap_or(0.0) * (1.0 - f)
                    + v.get(k + 1).copied().unwrap_or(0.0) * f
            };
            let fade = 1.0 - s * s;
            (get(&source.l) * fade, get(&source.r) * fade)
        } else {
            (0.0, 0.0)
        };
        track.l[start + i] = l;
        track.r[start + i] = r;
    }
}

/// The marimba melody: eight eighth notes for each of the four bars.
const PROGRESSION: [[f32; 8]; 4] = [
    [72.0, 69.0, 65.0, 69.0, 72.0, 74.0, 72.0, 69.0],
    [74.0, 69.0, 65.0, 69.0, 74.0, 76.0, 74.0, 69.0],
    [74.0, 70.0, 65.0, 70.0, 74.0, 77.0, 74.0, 70.0],
    [76.0, 72.0, 67.0, 72.0, 76.0, 79.0, 76.0, 72.0],
];
/// The bass note of each bar.
const ROOTS: [f32; 4] = [41.0, 38.0, 34.0, 36.0];
/// The pad chord of each bar.
const CHORDS: [[f32; 3]; 4] = [
    [65.0, 69.0, 72.0],
    [62.0, 65.0, 69.0],
    [58.0, 62.0, 65.0],
    [60.0, 64.0, 67.0],
];

/// The groove from `from` to `to`. More `energy` (0 to 1) makes the drums louder and adds
/// layers.
fn groove(music: &mut Track, from: f32, to: f32, energy: f32, seed: u32) {
    let eighth = BEAT / 2.0;
    let steps = ((to - from) / eighth) as usize;
    let k = soft_kick();
    for s in 0..steps {
        let t = from + s as f32 * eighth;
        let bar = (s / 8) % 4;
        let pos = s % 8;
        let accent = if pos == 0 || pos == 3 || pos == 6 {
            1.0
        } else {
            0.7
        };
        // The melody skips one note in odd bars. This gives it bounce.
        if !(pos == 5 && bar % 2 == 1) {
            music.add(
                t,
                &marimba(PROGRESSION[bar][pos], accent * 0.55),
                1.0,
                if pos % 2 == 0 { -0.25 } else { 0.25 },
            );
        }
        if pos == 0 || pos == 3 {
            music.add(t, &pizz(ROOTS[bar], 0.9, seed + s as u32), 1.0, -0.1);
        }
        if pos == 4 {
            music.add(
                t,
                &pizz(ROOTS[bar] + 7.0, 0.7, seed + s as u32 + 99),
                1.0,
                -0.1,
            );
        }
        if pos == 0 || pos == 4 {
            music.add(t, &k, 0.55 + 0.25 * energy, 0.0);
        }
        if pos == 2 || pos == 6 {
            music.add(t, &brush(seed + s as u32), 0.35 + 0.25 * energy, 0.15);
        }
        music.add(
            t,
            &shaker(seed + 700 + s as u32),
            if pos % 2 == 1 { 0.3 } else { 0.18 },
            0.4,
        );
        if energy > 0.5 && pos == 0 {
            let top = CHORDS[bar][2] + 12.0;
            music.add(t, &glock(top + 12.0, 0.25), 1.0, 0.3);
            music.add(t, &pad(&CHORDS[bar], BEAT * 4.0, 0.3), 1.0, 0.0);
        }
    }
}

fn score(show: &Show, length: f32) -> Track {
    let b = show.story.beats;
    let mut music = Track::new(length);
    // Waking up: a music box over a soft pad.
    let arp = [
        77.0, 81.0, 84.0, 88.0, 84.0, 81.0, 82.0, 86.0, 89.0, 86.0, 82.0, 81.0,
    ];
    let mut t = b.wake + 0.1;
    let mut i = 0;
    while t < b.work - 0.1 {
        music.add(
            t,
            &music_box(arp[i % arp.len()]),
            0.22,
            if i % 2 == 0 { -0.3 } else { 0.3 },
        );
        t += BEAT / 2.0;
        i += 1;
    }
    music.add(
        b.wake,
        &pad(&[53.0, 57.0, 60.0, 64.0], b.work - b.wake + 0.3, 1.2),
        1.0,
        0.0,
    );
    // Work: the groove. It gets more energy when the robot gets too confident.
    let cocky = b.drop - 2.0;
    groove(&mut music, b.work, cocky, 0.0, 10);
    groove(&mut music, cocky, b.drop + 1.0, 1.0, 400);
    // A fast run of notes up into the drop.
    for (k, n) in [72.0f32, 74.0, 76.0, 77.0, 79.0, 81.0, 82.0, 84.0]
        .iter()
        .enumerate()
    {
        music.add(b.drop - 0.5 + k as f32 * 0.055, &marimba(*n, 0.6), 1.0, 0.0);
    }
    tape_stop(&mut music, b.drop + 0.04, 0.55);

    // The awkward silence: crickets.
    let crickets_from = b.drop + 1.3;
    let crick = crickets(b.recover - crickets_from - 0.3, 5);
    music.add(crickets_from, &crick, 0.35, -0.6);
    music.add(
        crickets_from + 0.37,
        &crickets(b.recover - crickets_from - 0.6, 9),
        0.25,
        0.6,
    );
    // A sad slide down, like a trombone, under the sigh.
    music.add(b.recover - 1.25, &bassoon(46.0, 0.9, -3.0), 0.5, 0.0);

    // The careful recovery: quiet, short plucks and a low reed.
    let sneak = [53.0, 56.0, 60.0, 56.0, 55.0, 58.0, 61.0, 58.0];
    let mut t = b.recover + 0.15;
    let mut k = 0;
    while t < b.finish - 0.6 {
        if k % 4 != 3 {
            music.add(
                t,
                &pizz(sneak[k % sneak.len()], 0.55, 900 + k as u32),
                1.0,
                if k % 2 == 0 { -0.3 } else { 0.3 },
            );
        }
        if k % 4 == 0 {
            music.add(
                t,
                &bassoon(sneak[k % sneak.len()] - 12.0, 0.22, 0.0),
                0.35,
                0.0,
            );
        }
        t += BEAT;
        k += 1;
    }

    // Confident again, then the ta-da.
    groove(&mut music, b.finish, b.tada - 0.25, 1.0, 1200);
    music.add(b.tada - 0.25, &glock(84.0, 0.7), 1.0, 0.0);
    for n in [65.0f32, 69.0, 72.0, 77.0] {
        music.add(b.tada, &marimba(n, 0.6), 1.0, 0.0);
    }
    music.add(b.tada, &glock(89.0, 0.8), 1.0, 0.2);
    music.add(b.tada, &glock(84.0, 0.6), 1.0, -0.2);
    music.add(b.tada, &pad(&[53.0, 57.0, 60.0, 65.0], 3.0, 0.05), 0.8, 0.0);
    music.add(b.tada, &soft_kick(), 1.0, 0.0);
    music.add(b.tada, &pizz(29.0, 1.0, 77), 1.0, 0.0);

    let wet = reverb(&music, 0.78, 0.4, 0.22);
    let mut out = Track::new(length);
    out.mix(&music, 1.0);
    out.mix(&wet, 0.6);
    out.normalize(-1.0);
    out
}

// ------------------------------------------------------------------ The voice of the robot

/// A chirp that follows `pitch` (Hz over 0..1). A little FM makes it sound like a synthetic
/// voice.
fn chirp(length: f32, pitch: impl Fn(f32) -> f32, wobble: f32) -> Track {
    let mut phase = 0.0f32;
    let mut m = 0.0f32;
    let s = signal(length, |t| {
        let x = t / length;
        let f = pitch(x);
        m = (m + f * 2.01 / RATE).fract();
        phase = (phase + f * (1.0 + wobble * (t * 38.0).sin()) / RATE).fract();
        (phase * TAU + 0.6 * (m * TAU).sin()).sin()
            * smoothstep(0.0, 0.01, t)
            * (1.0 - smoothstep(length - 0.03, length, t))
    });
    let mut track = reverb(&mono(&s, -3.0), 0.6, 0.5, 0.15);
    track.normalize(-2.0);
    track
}

/// Mixes the parts at their start times and normalizes the result.
fn sequence(parts: &[(f32, Track)]) -> Track {
    let length = parts
        .iter()
        .map(|(t, p)| t + p.len() as f32 / RATE)
        .fold(0.0, f32::max);
    let mut out = Track::new(length);
    for (t, p) in parts {
        let offset = (*t * RATE) as usize;
        for i in 0..p.len() {
            if offset + i < out.len() {
                out.l[offset + i] += p.l[i];
                out.r[offset + i] += p.r[i];
            }
        }
    }
    out.normalize(-2.0);
    out
}

fn curious() -> Track {
    chirp(
        0.55,
        |x| {
            if x < 0.3 {
                700.0 + 800.0 * x / 0.3
            } else if x < 0.6 {
                1500.0 - 400.0 * (x - 0.3) / 0.3
            } else {
                1100.0 + 700.0 * ((x - 0.6) / 0.4).powi(2)
            }
        },
        0.02,
    )
}

fn happy() -> Track {
    chirp(
        0.4,
        |x| {
            if x < 0.75 {
                if ((x * 12.0) as u32).is_multiple_of(2) {
                    1300.0
                } else {
                    1750.0
                }
            } else {
                1500.0 + 1200.0 * (x - 0.75) * 4.0
            }
        },
        0.0,
    )
}

fn uh_oh() -> Track {
    sequence(&[
        (0.0, chirp(0.2, |x| 900.0 - 100.0 * x, 0.01)),
        (0.26, chirp(0.4, |x| 640.0 - 180.0 * x, 0.03)),
    ])
}

fn sigh() -> Track {
    let mut out = chirp(1.0, |x| 560.0 - 380.0 * x.powf(0.7), 0.015);
    let mut noise = Noise(3);
    let mut band = Biquad::new(Kind::Band, 900.0, 1.0);
    let breath = signal(1.0, |t| {
        band.run(noise.next()) * attack_decay(t, 0.25, 2.5) * 0.4
    });
    out.add(0.0, &breath, 1.0, 0.0);
    out.normalize(-2.0);
    out
}

fn proud() -> Track {
    sequence(&[
        (0.0, chirp(0.08, |_| 800.0, 0.0)),
        (0.09, chirp(0.08, |_| 1000.0, 0.0)),
        (0.18, chirp(0.08, |_| 1200.0, 0.0)),
        (0.27, chirp(0.3, |x| 1600.0 + 300.0 * x, 0.03)),
    ])
}

fn tada() -> Track {
    sequence(&[
        (0.0, chirp(0.1, |_| 1200.0, 0.0)),
        (
            0.14,
            chirp(0.45, |x| 1800.0 + 100.0 * (x * 20.0).sin(), 0.0),
        ),
    ])
}

fn boot() -> Track {
    let sweep = chirp(0.5, |x| 300.0 + 900.0 * x * x, 0.0);
    sequence(&[
        (0.0, sweep),
        (0.55, chirp(0.06, |_| 1400.0, 0.0)),
        (0.66, chirp(0.06, |_| 1400.0, 0.0)),
        (0.77, chirp(0.1, |_| 1900.0, 0.0)),
    ])
}

// ------------------------------------------------------------------ Foley

/// A short click of band passed noise.
fn click(seed: u32, freq: f32, decay_rate: f32, length: f32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut band = Biquad::new(Kind::Band, freq, 3.0);
    signal(length, |t| {
        band.run(noise.next()) * decay(t, decay_rate) * 4.0
    })
}

fn blink() -> Track {
    let mut track = mono(&click(5, 5200.0, 300.0, 0.05), -6.0);
    track.add(0.04, &click(6, 4200.0, 400.0, 0.04), 0.6, 0.0);
    track
}

fn grip() -> Track {
    let mut noise = Noise(51);
    let mut high = Biquad::new(Kind::High, 4000.0, 0.7);
    let mut snap = Biquad::new(Kind::Band, 2800.0, 8.0);
    let s = signal(0.35, |t| {
        let n = noise.next();
        high.run(n) * attack_decay(t, 0.01, 18.0) * 0.5
            + snap.run(if t > 0.07 {
                n * decay(t - 0.07, 900.0)
            } else {
                0.0
            }) * 5.0
    });
    reverb(&mono(&s, -2.0), 0.6, 0.5, 0.12)
}

/// A lacquered letter that the robot puts down on concrete.
fn place() -> Track {
    let mut noise = Noise(77);
    let mut body = Biquad::new(Kind::Band, 520.0, 4.0);
    let mut sub = Sine::default();
    let s = signal(0.6, |t| {
        let n = noise.next();
        body.run(n * decay(t, 160.0)) * 7.0 * decay(t, 22.0)
            + sub.next(70.0 + 40.0 * decay(t, 40.0)) * decay(t, 14.0) * 0.8
    });
    let mut track = reverb(&mono(&s, -1.0), 0.75, 0.45, 0.15);
    track.normalize(-1.0);
    track
}

/// A hard hit with the ring of a thin plate.
fn clatter() -> Track {
    let modes = [(1250.0f32, 9.0f32), (2930.0, 14.0), (4710.0, 22.0)];
    let mut noise = Noise(31);
    let mut bands: Vec<Biquad> = modes
        .iter()
        .map(|(f, _)| Biquad::new(Kind::Band, *f, 25.0))
        .collect();
    let mut sub = Sine::default();
    let s = signal(0.5, |t| {
        let n = noise.next() * decay(t, 500.0);
        let ring: f32 = bands
            .iter_mut()
            .zip(modes)
            .map(|(b, (_, d))| b.run(n) * decay(t, d) * 9.0)
            .sum();
        ring + n * 0.6 + sub.next(90.0) * decay(t, 25.0) * 0.6
    });
    let mut track = reverb(&mono(&s, -1.0), 0.7, 0.4, 0.18);
    track.normalize(-1.0);
    track
}

fn pat() -> Track {
    let mut track = mono(&click(8, 900.0, 60.0, 0.15), -3.0);
    track.add(0.0, &click(9, 2400.0, 200.0, 0.05), 0.4, 0.0);
    track
}

fn whoosh() -> Track {
    let mut noise = Noise(12);
    let mut band = Biquad::new(Kind::Band, 400.0, 1.2);
    let s = signal(0.5, |t| {
        band.set(Kind::Band, 300.0 + 1500.0 * (t / 0.5 * PI).sin(), 1.2);
        band.run(noise.next()) * (t / 0.5 * PI).sin().powi(2)
    });
    mono(&s, -3.0)
}

fn scratch() -> Track {
    let mut noise = Noise(44);
    let mut band = Biquad::new(Kind::Band, 800.0, 4.0);
    let mut saw = Saw::default();
    let s = signal(0.42, |t| {
        // The record moves back and forward two times. The second time is slower.
        let x = t / 0.42;
        let pitch = if x < 0.35 {
            (x / 0.35 * PI).sin()
        } else {
            ((x - 0.35) / 0.65 * PI).sin() * 0.7
        };
        band.set(Kind::Band, 400.0 + 2600.0 * pitch, 5.0);
        (band.run(noise.next()) * 2.0 + saw.next(80.0 + 300.0 * pitch) * 0.3) * pitch.abs().sqrt()
    });
    mono(&s, -1.0)
}

/// The lab lights: heavy contactors close, then the tubes start to hum.
fn lights() -> Track {
    let mut track = Track::new(2.0);
    for (k, at) in [0.0f32, 0.06, 0.12, 0.2, 0.27].iter().enumerate() {
        let thunk = {
            let mut noise = Noise(100 + k as u32);
            let mut low = Biquad::new(Kind::Low, 700.0, 0.7);
            let mut sub = Sine::default();
            signal(0.3, |t| {
                low.run(noise.next()) * decay(t, 40.0) * 2.0 + sub.next(55.0) * decay(t, 20.0)
            })
        };
        track.add(
            *at,
            &thunk,
            if k % 2 == 0 { 1.0 } else { 0.5 },
            if k % 2 == 0 { -0.3 } else { 0.3 },
        );
    }
    let mut hum = [Sine::default(), Sine::default(), Sine::default()];
    let buzz = signal(1.6, |t| {
        let s: f32 = hum
            .iter_mut()
            .zip([120.0, 240.0, 360.0])
            .map(|(o, f)| o.next(f))
            .sum();
        s * smoothstep(0.3, 0.8, t) * (1.0 - smoothstep(1.0, 1.6, t)) * 0.08
    });
    track.add(0.3, &buzz, 1.0, 0.0);
    let mut track = reverb(&track, 0.85, 0.3, 0.3);
    track.normalize(-1.0);
    track
}

// ------------------------------------------------------------------ Beds

/// The room tone: a low rumble, a little air and a mains hum.
fn room(length: f32) -> Track {
    let mut track = Track::new(length);
    for (channel, seed) in [(0usize, 1u32), (1, 2)] {
        let mut noise = Noise(seed);
        let mut low = Biquad::new(Kind::Low, 140.0, 0.7);
        let mut high = Biquad::new(Kind::High, 5000.0, 0.7);
        let mut hum = Sine::default();
        let s = signal(length, |t| {
            let n = noise.next();
            low.run(n) * 1.5
                + high.run(n) * 0.04
                + hum.next(120.0) * 0.02 * (0.8 + 0.2 * (t * 0.3).sin())
        });
        if channel == 0 {
            track.l = s;
        } else {
            track.r = s;
        }
    }
    track.normalize(-6.0);
    track
}

/// The motor of each joint: low motors for J1 to J3 and small, high motors for the wrist.
/// Their level and pitch follow the joint speeds of the animation.
fn servos(show: &Show, length: f32) -> Track {
    let rate = 240.0;
    let samples = (length * rate) as usize + 2;
    let speeds: Vec<[f32; 6]> = (0..samples)
        .map(|i| {
            let t = i as f32 / rate;
            let a = show.story.track.sample(t).q;
            let b = show.story.track.sample(t + 1.0 / rate).q;
            std::array::from_fn(|j| (b.q[j] - a.q[j]).abs() * rate)
        })
        .collect();
    let base = [70.0f32, 95.0, 120.0, 330.0, 380.0, 450.0];
    let gain = [1.0f32, 1.0, 0.9, 0.5, 0.55, 0.45];
    let mut track = Track::new(length);
    for j in 0..6 {
        let mut a = Sine::default();
        let mut b = Sine::default();
        let mut noise = Noise(60 + j as u32);
        let mut band = Biquad::new(Kind::Band, base[j] * 6.0, 4.0);
        let mut level = 0.0f32;
        let s = signal(length, |t| {
            let x = t * rate;
            let i = (x as usize).min(samples - 2);
            let speed = speeds[i][j] + (speeds[i + 1][j] - speeds[i][j]) * (x - i as f32);
            let target = (speed / 2.5).min(1.0);
            level += (target - level) * 0.003;
            let f = base[j] * (1.0 + 0.7 * (speed / 3.0).min(1.5));
            let gears = band.run(noise.next()) * 0.5;
            (a.next(f) * 0.6 + b.next(f * 2.02) * 0.3 + gears) * level * gain[j]
        });
        track.add(0.0, &s, 1.0, (j as f32 - 2.5) * 0.08);
    }
    track.normalize(-3.0);
    track
}

fn main() -> std::io::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    std::fs::create_dir_all(&dir)?;
    for old in std::fs::read_dir(&dir)?.flatten() {
        if old.path().extension().is_some_and(|e| e == "wav") {
            std::fs::remove_file(old.path())?;
        }
    }
    let show = Show::new();
    let length = show.duration() + 1.5;
    for (name, track) in [
        ("music.wav", score(&show, length)),
        ("room.wav", room(length)),
        ("servos.wav", servos(&show, length)),
        ("lights.wav", lights()),
        ("boot.wav", boot()),
        ("blink.wav", blink()),
        ("curious.wav", curious()),
        ("happy.wav", happy()),
        ("grip.wav", grip()),
        ("place.wav", place()),
        ("whoosh.wav", whoosh()),
        ("scratch.wav", scratch()),
        ("clatter.wav", clatter()),
        ("uhoh.wav", uh_oh()),
        ("sigh.wav", sigh()),
        ("pat.wav", pat()),
        ("proud.wav", proud()),
        ("tada.wav", tada()),
    ] {
        track.write(&dir.join(name))?;
        println!(
            "wrote dynamic_media/{name} ({:.2} s)",
            track.len() as f32 / RATE
        );
    }
    Ok(())
}
