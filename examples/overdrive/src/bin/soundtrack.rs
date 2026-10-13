//! Synthesizes the soundtrack of OVERDRIVE into `dynamic_media/`: a 120 BPM music bed and the
//! sound effects. The video places the effects from the event list of the animation.
//!
//! `cargo run --release -p overdrive-example --bin overdrive-soundtrack`

use std::f32::consts::{PI, TAU};
use std::path::Path;

use fframes_3d::math::smoothstep;
use overdrive_example::story::*;

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

/// Samples `f(t, i)` for `seconds`. `i` is the index of the sample.
fn signal(seconds: f32, mut f: impl FnMut(f32, usize) -> f32) -> Vec<f32> {
    (0..(seconds * RATE) as usize)
        .map(|i| f(i as f32 / RATE, i))
        .collect()
}

// ------------------------------------------------------------------ Instruments

/// A kick drum. More `punch` makes the body longer.
fn kick(punch: f32) -> Vec<f32> {
    let mut osc = Sine::default();
    let mut noise = Noise(7);
    signal(0.55, |t, _| {
        let f = 46.0 + 150.0 * decay(t, 28.0) + 40.0 * decay(t, 120.0);
        let body = osc.next(f) * decay(t, 6.5 - punch * 2.0);
        let click = noise.next() * decay(t, 400.0) * 0.4;
        ((body * 1.6 + click).tanh()) * 0.95
    })
}

/// A hand clap: four short noise bursts and a body.
fn clap() -> Vec<f32> {
    let mut noise = Noise(99);
    let mut band = Biquad::new(Kind::Band, 1500.0, 0.9);
    let mut body = Sine::default();
    signal(0.4, |t, _| {
        let bursts = [0.0f32, 0.011, 0.023, 0.036];
        let env: f32 = bursts
            .iter()
            .map(|&b| {
                if t >= b {
                    decay(t - b, if b == 0.036 { 14.0 } else { 120.0 })
                } else {
                    0.0
                }
            })
            .sum();
        band.run(noise.next()) * env * 1.4 + body.next(185.0) * decay(t, 30.0) * 0.3
    })
}

/// An open or closed hi-hat.
fn hat(open: bool, seed: u32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut high = Biquad::new(Kind::High, 7500.0, 0.7);
    let rate = if open { 9.0 } else { 55.0 };
    signal(if open { 0.5 } else { 0.12 }, |t, _| {
        high.run(noise.next()) * decay(t, rate) * 0.5
    })
}

/// A crash cymbal.
fn crash(seed: u32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut high = Biquad::new(Kind::High, 3500.0, 0.6);
    let mut ring = [Sine::default(), Sine::default(), Sine::default()];
    signal(2.5, |t, _| {
        let metal: f32 = ring
            .iter_mut()
            .zip([3150.0, 4720.0, 6890.0])
            .map(|(o, f)| o.next(f))
            .sum();
        (high.run(noise.next()) + metal * 0.05) * attack_decay(t, 0.002, 1.8) * 0.6
    })
}

/// A sub bass boom that falls from `from` to `to` Hz.
fn sub_boom(seconds: f32, from: f32, to: f32) -> Vec<f32> {
    let mut osc = Sine::default();
    signal(seconds, |t, _| {
        let f = to + (from - to) * decay(t, 3.0);
        (osc.next(f) * 1.4).tanh() * attack_decay(t, 0.004, 2.2)
    })
}

/// Seven detuned saws, mixed.
fn supersaw(freq: f32, saws: &mut [Saw; 7]) -> f32 {
    let detune = [-0.11, -0.063, -0.019, 0.0, 0.021, 0.061, 0.107];
    saws.iter_mut()
        .zip(detune)
        .map(|(s, d)| s.next(freq * (1.0 + d * 0.06)))
        .sum::<f32>()
        / 4.0
}

/// A short supersaw chord with a closing filter.
fn chord_stab(notes: &[f32], length: f32, cutoff: f32) -> Vec<f32> {
    let mut saws: Vec<[Saw; 7]> = notes.iter().map(|_| Default::default()).collect();
    let mut low = Biquad::new(Kind::Low, cutoff, 0.8);
    signal(length + 0.3, |t, _| {
        let raw: f32 = notes
            .iter()
            .zip(saws.iter_mut())
            .map(|(n, s)| supersaw(note(*n), s))
            .sum();
        low.set(
            Kind::Low,
            cutoff * (0.35 + 0.65 * decay(t, 6.0)) + 200.0,
            0.8,
        );
        low.run(raw) * attack_decay(t, 0.004, 1.0 / length.max(0.05) * 2.2) * 0.35
    })
}

/// Noise and a saw that rise for `seconds`.
fn riser(seconds: f32, seed: u32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut band = Biquad::new(Kind::Band, 400.0, 1.5);
    let mut saw = Saw::default();
    signal(seconds, |t, _| {
        let x = t / seconds;
        band.set(Kind::Band, 300.0 + 7000.0 * x * x, 2.0);
        let tone = saw.next(110.0 * 2f32.powf(x * 2.5)) * 0.15 * x;
        (band.run(noise.next()) * 1.5 + tone) * x * x * 0.8
    })
}

// ------------------------------------------------------------------ The music bed

fn music() -> Track {
    let length = DURATION + 0.5;
    let mut drums = Track::new(length);
    let mut synths = Track::new(length);
    let k = kick(0.0);
    let k_big = kick(1.0);
    let c = clap();
    let crash_hit = crash(5);

    // Intro: a pulsing bass that opens up, hi-hats and a riser into the jump.
    let mut saw = Saw::default();
    let mut saw2 = Saw::default();
    let mut low = Biquad::new(Kind::Low, 300.0, 1.2);
    let intro = signal(LAUNCH, |t, _| {
        let step = (t / (BEAT / 4.0)).floor();
        let in_step = t - step * BEAT / 4.0;
        let n = if step as i32 % 8 == 6 { 41.0 } else { 38.0 };
        low.set(
            Kind::Low,
            250.0 + 2400.0 * (t / LAUNCH).powi(2) * decay(in_step, 9.0) + 200.0,
            4.0,
        );
        let raw = saw.next(note(n - 12.0)) + 0.6 * saw2.next(note(n - 12.0) * 1.005);
        (low.run(raw) * decay(in_step, 7.0) * 1.6).tanh() * 0.55
    });
    synths.add(0.0, &intro, 1.0, 0.0);
    for i in 0..(LAUNCH / (BEAT / 4.0)) as usize {
        let t = i as f32 * BEAT / 4.0;
        let level = 0.12 + 0.35 * t / LAUNCH;
        drums.add(
            t,
            &hat(false, 11 + i as u32),
            level,
            if i % 2 == 0 { -0.25 } else { 0.25 },
        );
    }
    drums.add(LAUNCH - 1.2, &riser(1.2, 3), 0.8, 0.0);
    for b in [0.0, 1.0, 2.0, 3.0] {
        drums.add(b * BEAT, &k, 0.55, 0.0);
    }

    // Bullet time: a hit at the jump, a slow heartbeat, a pad and a snare roll into the drop.
    drums.add(LAUNCH, &k_big, 1.0, 0.0);
    drums.add(LAUNCH, &crash_hit, 0.5, 0.0);
    drums.add(LAUNCH, &sub_boom(1.6, 90.0, 38.0), 0.9, 0.0);
    drums.add(LAUNCH + 1.0, &k, 0.5, 0.0);
    let pad_len = LAND - LAUNCH;
    let mut pads: Vec<[Saw; 7]> = vec![Default::default(); 4];
    let mut pad_low = Biquad::new(Kind::Low, 900.0, 0.7);
    let pad = signal(pad_len, |t, _| {
        let notes = [50.0, 53.0, 57.0, 62.0];
        let raw: f32 = notes
            .iter()
            .zip(pads.iter_mut())
            .map(|(n, s)| supersaw(note(*n), s))
            .sum();
        pad_low.set(Kind::Low, 500.0 + 2500.0 * (t / pad_len).powi(2), 0.7);
        pad_low.run(raw)
            * smoothstep(0.0, 0.6, t)
            * (1.0 - smoothstep(pad_len - 0.08, pad_len - 0.02, t))
            * 0.16
    });
    synths.add(LAUNCH, &pad, 1.0, 0.0);
    let roll_start = LAND - 1.0;
    let mut t = roll_start;
    let mut i = 0;
    while t < LAND - 0.06 {
        let x = (t - roll_start) / (LAND - roll_start);
        drums.add(
            t,
            &c,
            0.15 + 0.5 * x * x,
            if i % 2 == 0 { -0.1 } else { 0.1 },
        );
        t += if x < 0.5 {
            BEAT / 2.0
        } else if x < 0.8 {
            BEAT / 4.0
        } else {
            BEAT / 8.0
        };
        i += 1;
    }
    drums.add(LAND - 1.0, &riser(0.97, 9), 0.9, 0.0);

    // The groove: from the landing to the build, and again from the shot to the title.
    let groove = |drums: &mut Track, synths: &mut Track, from: f32, to: f32, seed: u32| {
        let beats = ((to - from) / BEAT).round() as i32;
        for b in 0..beats {
            let t = from + b as f32 * BEAT;
            drums.add(t, &k, 0.95, 0.0);
            if b % 2 == 1 {
                drums.add(t, &c, 0.55, 0.0);
            }
            for s in 0..4 {
                let level = [0.22, 0.12, 0.3, 0.12][s];
                let open = s == 2 && b % 4 == 3;
                drums.add(
                    t + s as f32 * BEAT / 4.0,
                    &hat(open, seed + (b as u32) * 4 + s as u32),
                    level,
                    if s % 2 == 0 { 0.3 } else { -0.3 },
                );
            }
        }
        // Bass: distorted sixteenth notes with an accent pattern.
        let mut saw = Saw::default();
        let mut low = Biquad::new(Kind::Low, 600.0, 2.5);
        let riff = [38.0, 38.0, 50.0, 38.0, 38.0, 48.0, 38.0, 41.0];
        let bass = signal(to - from, |t, _| {
            let step = (t / (BEAT / 4.0)).floor();
            let in_step = t - step * BEAT / 4.0;
            let bar = ((t / (BEAT * 4.0)).floor() as usize) % 2;
            let mut n = riff[step as usize % 8];
            if bar == 1 && (step as usize % 16) >= 12 {
                n += 3.0;
            }
            low.set(Kind::Low, 180.0 + 1700.0 * decay(in_step, 18.0), 3.0);
            (low.run(saw.next(note(n - 12.0))) * 2.2).tanh() * decay(in_step, 4.0) * 0.5
        });
        synths.add(from, &bass, 1.0, 0.0);
        // Chord stabs between the beats.
        let chords = [
            [62.0, 65.0, 69.0],
            [58.0, 62.0, 65.0],
            [60.0, 64.0, 67.0],
            [57.0, 60.0, 64.0],
        ];
        for b in 0..beats {
            let t = from + b as f32 * BEAT + BEAT / 2.0;
            let chord = chords[(b / 4) as usize % 4];
            synths.add(
                t,
                &chord_stab(&chord, 0.12, 4200.0),
                0.5,
                if b % 2 == 0 { -0.3 } else { 0.3 },
            );
        }
    };
    drums.add(LAND, &k_big, 1.0, 0.0);
    drums.add(LAND, &crash_hit, 0.8, 0.0);
    groove(&mut drums, &mut synths, LAND, AIM, 100);

    // The build while the cannon charges: two kicks, then a roll and a riser.
    for b in 0..2 {
        drums.add(AIM + b as f32 * BEAT, &k, 0.8, 0.0);
    }
    let roll_start = CHARGE;
    let mut t = roll_start;
    let mut i = 0;
    while t < FIRE - 0.05 {
        let x = (t - roll_start) / (FIRE - roll_start);
        drums.add(
            t,
            &c,
            0.12 + 0.45 * x * x,
            if i % 2 == 0 { -0.15 } else { 0.15 },
        );
        t += if x < 0.5 { BEAT / 4.0 } else { BEAT / 8.0 };
        i += 1;
    }
    drums.add(AIM, &riser(FIRE - AIM - 0.03, 21), 0.7, 0.0);

    // The drop at the shot.
    drums.add(FIRE, &k_big, 1.0, 0.0);
    drums.add(FIRE, &crash(6), 0.8, 0.0);
    groove(&mut drums, &mut synths, FIRE, TITLE, 300);
    drums.add(TITLE - 0.5, &riser(0.48, 31), 0.6, 0.0);

    // The title chord.
    drums.add(TITLE, &k_big, 1.0, 0.0);
    drums.add(TITLE, &crash(7), 0.9, 0.0);
    drums.add(TITLE, &sub_boom(2.5, 80.0, 36.0), 0.9, 0.0);
    synths.add(
        TITLE,
        &chord_stab(&[50.0, 57.0, 62.0, 65.0, 69.0], 1.6, 3000.0),
        0.9,
        0.0,
    );

    let wet = reverb(&synths, 0.84, 0.3, 0.35);
    let mut mix = Track::new(length);
    mix.mix(&drums, 1.0);
    mix.mix(&wet, 1.0);
    mix.normalize(-1.0);
    mix
}

// ------------------------------------------------------------------ Sound effects

/// A centered track from a mono signal, normalized to `peak_db`.
fn mono(signal: Vec<f32>, peak_db: f32) -> Track {
    let mut t = Track::new(signal.len() as f32 / RATE);
    t.add(0.0, &signal, 1.0, 0.0);
    t.normalize(peak_db);
    t
}

/// A heavy metal part that locks into place: a thump, a click and an inharmonic ring.
fn clank(base: f32, seed: u32) -> Track {
    let mut noise = Noise(seed);
    let modes = [1.0, 2.76, 5.40, 8.93, 13.34];
    let mut bands: Vec<Biquad> = modes
        .iter()
        .map(|m| Biquad::new(Kind::Band, base * m, 30.0))
        .collect();
    let mut thump = Sine::default();
    let s = signal(0.5, |t, _| {
        let excite = noise.next() * decay(t, 260.0);
        let ring: f32 = bands
            .iter_mut()
            .enumerate()
            .map(|(i, b)| b.run(excite) * decay(t, 9.0 + i as f32 * 4.0) * 6.0)
            .sum();
        let low = thump.next(70.0 + 60.0 * decay(t, 40.0)) * decay(t, 18.0) * 0.9;
        (ring + low + excite * 0.4).tanh()
    });
    let mut track = reverb(&mono(s, -1.0), 0.6, 0.4, 0.18);
    track.normalize(-1.0);
    track
}

/// A servo whine that steps through the gears, with gear ticks.
fn servo() -> Track {
    let mut saw = Saw::default();
    let mut noise = Noise(77);
    let mut band = Biquad::new(Kind::Band, 900.0, 3.0);
    let s = signal(1.8, |t, _| {
        let step = (t * 11.0).floor();
        let f = 160.0 + 70.0 * (step * 1.7).sin().abs() + 50.0 * t;
        let whine = band.run(saw.next(f) + saw.next(f * 2.01) * 0.3);
        let tick = noise.next() * decay((t * 42.0).fract() / 42.0, 900.0) * 0.5;
        (whine * 1.5 + tick) * smoothstep(0.0, 0.05, t) * 0.8
    });
    mono(s, -1.0)
}

/// A whoosh and a low thump for the jump.
fn launch() -> Track {
    let mut noise = Noise(5);
    let mut band = Biquad::new(Kind::Band, 300.0, 1.2);
    let mut sub = Sine::default();
    let s = signal(1.0, |t, _| {
        band.set(
            Kind::Band,
            200.0 + 2600.0 * (1.0 - (t - 0.25).abs() * 3.0).max(0.0),
            1.4,
        );
        let whoosh = band.run(noise.next()) * attack_decay(t, 0.2, 4.0) * 2.0;
        whoosh + sub.next(55.0 + 40.0 * decay(t, 8.0)) * decay(t, 7.0) * 0.8
    });
    let mut track = Track::new(1.0);
    track.add(0.0, &s, 1.0, 0.0);
    reverb(&track, 0.7, 0.4, 0.2)
}

/// The landing: a crack, a boom, debris and a metal ring.
fn impact_fx() -> Track {
    let mut noise = Noise(13);
    let mut low = Biquad::new(Kind::Low, 2000.0, 0.7);
    let mut sub = Sine::default();
    let ring = clank(140.0, 41);
    let s = signal(2.8, |t, i| {
        low.set(Kind::Low, 300.0 + 5000.0 * decay(t, 9.0), 0.7);
        let crack = low.run(noise.next()) * decay(t, 5.0) * 1.2;
        let boom = sub.next(32.0 + 70.0 * decay(t, 6.0)) * attack_decay(t, 0.003, 1.6) * 1.5;
        // Debris: a few clicks.
        let debris = if (i % 997) < 3 && t > 0.15 {
            noise.next() * decay(t, 2.5) * 0.8
        } else {
            0.0
        };
        (crack + boom + debris).tanh()
    });
    let mut track = Track::new(2.8);
    track.add(0.0, &s, 1.0, 0.0);
    track.mix(&ring, 0.5);
    let mut track = reverb(&track, 0.85, 0.35, 0.3);
    track.normalize(-0.5);
    track
}

/// A rising sweep and a ding.
fn power_up() -> Track {
    let mut a = Saw::default();
    let mut b = Sine::default();
    let mut low = Biquad::new(Kind::Low, 400.0, 3.0);
    let s = signal(1.4, |t, _| {
        let x = (t / 0.7).min(1.0);
        let f = 70.0 * 2f32.powf(x * 4.2);
        low.set(Kind::Low, f * 3.0 + 200.0, 4.0);
        let sweep = low.run(a.next(f)) * (0.3 + 0.7 * x) * (1.0 - smoothstep(0.7, 1.4, t));
        let ding = b.next(1760.0) * attack_decay(t - 0.7, 0.005, 3.5) * 0.5;
        sweep * 0.8 + ding
    });
    let mut track = reverb(&mono(s, -1.0), 0.8, 0.3, 0.3);
    track.normalize(-1.0);
    track
}

/// A short two tone blip for the callouts.
fn blip() -> Track {
    let mut osc = Sine::default();
    let s = signal(0.3, |t, _| {
        let f = if t < 0.045 { 1900.0 } else { 2500.0 };
        osc.next(f) * (if t < 0.09 { 1.0 } else { 0.0 }) * smoothstep(0.0, 0.003, t) * 0.6
    });
    reverb(&mono(s, -2.0), 0.5, 0.5, 0.2)
}

/// A hiss and three clanks for the cannon that opens.
fn cannon_open() -> Track {
    let mut noise = Noise(55);
    let mut high = Biquad::new(Kind::High, 2500.0, 0.7);
    let hiss = signal(0.7, |t, _| {
        high.run(noise.next()) * attack_decay(t, 0.05, 5.0) * 0.4
    });
    let mut track = Track::new(0.8);
    track.add(0.0, &hiss, 1.0, 0.0);
    for (i, at) in [0.0f32, 0.12, 0.3].iter().enumerate() {
        let c = clank(380.0 + i as f32 * 140.0, 60 + i as u32);
        track.add(*at, &c.l, 0.7, 0.0);
    }
    track.normalize(-1.0);
    track
}

/// A rising tone with crackle while the cannon charges.
fn charge() -> Track {
    let length = FIRE - CHARGE;
    let mut a = Sine::default();
    let mut b = Saw::default();
    let mut noise = Noise(88);
    let mut band = Biquad::new(Kind::Band, 2000.0, 4.0);
    let s = signal(length, |t, _| {
        let x = t / length;
        let f = 220.0 * 2f32.powf(x * 3.4) * (1.0 + 0.01 * (t * 40.0 * (1.0 + 3.0 * x)).sin());
        band.set(Kind::Band, f * 2.0, 6.0);
        let crackle = band.run(noise.next()) * x * x * 0.6;
        (a.next(f) * 0.6 + b.next(f * 0.5) * 0.15 + crackle) * (0.15 + 0.85 * x * x)
    });
    mono(s, -1.0)
}

/// The shot: a zap, noise and a boom.
fn blast() -> Track {
    let mut saw = Saw::default();
    let mut noise = Noise(31);
    let mut low = Biquad::new(Kind::Low, 3000.0, 0.8);
    let mut sub = Sine::default();
    let s = signal(2.2, |t, _| {
        let zap = saw.next(120.0 + 2400.0 * decay(t, 14.0)) * decay(t, 5.0) * 0.8;
        low.set(Kind::Low, 400.0 + 6000.0 * decay(t, 6.0), 0.7);
        let noise = low.run(noise.next()) * decay(t, 3.0) * 1.2;
        let boom = sub.next(36.0 + 90.0 * decay(t, 10.0)) * attack_decay(t, 0.003, 2.0) * 1.4;
        (zap + noise + boom).tanh()
    });
    let mut track = Track::new(2.2);
    track.add(0.0, &s, 1.0, 0.0);
    let mut track = reverb(&track, 0.88, 0.3, 0.35);
    track.normalize(-0.5);
    track
}

/// The car lands on the road.
fn thud() -> Track {
    let mut sub = Sine::default();
    let mut noise = Noise(3);
    let mut low = Biquad::new(Kind::Low, 600.0, 0.7);
    let s = signal(0.9, |t, _| {
        (sub.next(45.0 + 60.0 * decay(t, 12.0)) * decay(t, 6.0) * 1.4
            + low.run(noise.next()) * decay(t, 14.0))
        .tanh()
    });
    mono(s, -1.0)
}

/// An engine roar: harmonics of the firing frequency and a rough growl.
fn engine_tone(rpm: impl Fn(f32) -> f32, seconds: f32, seed: u32) -> Vec<f32> {
    let mut phase = 0.0f32;
    let mut noise = Noise(seed);
    let mut low = Biquad::new(Kind::Low, 1800.0, 0.9);
    signal(seconds, |t, _| {
        // The firing frequency of a V8.
        let f = rpm(t) / 60.0 * 4.0;
        phase = (phase + f / RATE).fract();
        let mut s = 0.0;
        for h in 1..9 {
            s += (phase * TAU * h as f32).sin() / h as f32 * if h % 2 == 0 { 0.7 } else { 1.0 };
        }
        let rough = noise.next() * 0.25 * (phase * TAU * 0.5).sin().abs();
        (low.run(s + rough) * 1.8).tanh() * 0.6
    })
}

/// The engine before the jump.
fn engine() -> Track {
    let length = LAUNCH + 0.5;
    // The engine revs up, changes gear at 1.1 s and revs up again.
    let tone = engine_tone(
        |t| {
            if t < 1.1 {
                4200.0 + 2400.0 * t / 1.1
            } else {
                4600.0 + 2600.0 * ((t - 1.1) / 1.4).min(1.0)
            }
        },
        length,
        9,
    );
    let mut track = Track::new(length);
    track.add(0.0, &tone, 1.0, 0.0);
    track.normalize(-1.0);
    track
}

/// The car passes on the left at 0.3 s into the file. There the pitch falls, and the level
/// and the pan are at their peak.
fn pass_by() -> Track {
    let length = 1.8;
    let pass = 0.3;
    let tone = engine_tone(
        |t| {
            6400.0
                * if t < pass {
                    1.12
                } else {
                    0.86 + 0.26 * (-(t - pass) * 18.0).exp()
                }
        },
        length,
        4,
    );
    let mut noise = Noise(8);
    let mut band = Biquad::new(Kind::Band, 800.0, 0.8);
    let mut track = Track::new(length);
    for (i, s) in tone.iter().enumerate() {
        let t = i as f32 / RATE;
        let distance = ((t - pass) * 30.0).abs().max(2.0);
        let level = 2.5 / distance;
        let wind = band.run(noise.next()) * level * 0.8;
        let pan = (-(t - pass) * 3.0).tanh() * 0.4 - 0.5;
        let angle = (pan + 1.0) * PI / 4.0;
        let x = (s + wind) * level;
        track.l[i] += x * angle.cos();
        track.r[i] += x * angle.sin();
    }
    track.normalize(-1.0);
    track
}

/// A gong for the title.
fn title_hit() -> Track {
    let mut gong: Vec<Sine> = vec![Sine::default(); 6];
    let partials = [1.0, 2.32, 3.98, 5.4, 6.8, 8.9];
    let s = signal(3.0, |t, _| {
        gong.iter_mut()
            .zip(partials)
            .enumerate()
            .map(|(i, (o, p))| {
                o.next(73.0 * p) * decay(t, 1.2 + i as f32 * 0.6) / (1.0 + i as f32 * 0.5)
            })
            .sum::<f32>()
            * smoothstep(0.0, 0.004, t)
    });
    let mut track = reverb(&mono(s, -2.0), 0.9, 0.25, 0.45);
    track.normalize(-1.0);
    track
}

fn main() -> std::io::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    std::fs::create_dir_all(&dir)?;
    let write = |name: &str, track: Track| -> std::io::Result<()> {
        track.write(&dir.join(name))?;
        println!(
            "wrote dynamic_media/{name} ({:.2} s)",
            track.len() as f32 / RATE
        );
        Ok(())
    };
    write("music.wav", music())?;
    for (i, base) in [310.0f32, 420.0, 520.0, 380.0].iter().enumerate() {
        write(&format!("clank{}.wav", i + 1), clank(*base, 17 + i as u32))?;
    }
    write("servo.wav", servo())?;
    write("launch.wav", launch())?;
    write("impact.wav", impact_fx())?;
    write("powerup.wav", power_up())?;
    write("blip.wav", blip())?;
    write("cannon_open.wav", cannon_open())?;
    write("charge.wav", charge())?;
    write("blast.wav", blast())?;
    write("thud.wav", thud())?;
    write("engine.wav", engine())?;
    write("passby.wav", pass_by())?;
    write("title.wav", title_hit())?;
    Ok(())
}
