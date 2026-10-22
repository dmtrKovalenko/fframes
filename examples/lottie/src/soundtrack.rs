//! The music of the video, synthesized when the video starts: a 120 BPM track in A minor.
//! Its hits, plucks and keystrokes use the timing in `beat.rs`.

use std::borrow::Cow;
use std::f32::consts::{PI, TAU};

use fframes::media::PreloadedAudioData;

use crate::beat::*;

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

#[derive(Clone, Copy)]
enum Kind {
    Low,
    High,
    Band,
}

/// An RBJ biquad filter.
#[derive(Default, Clone)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
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
        let b = match kind {
            Kind::Low => [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
            Kind::High => [
                f32::midpoint(1.0, cos),
                -(1.0 + cos),
                f32::midpoint(1.0, cos),
            ],
            Kind::Band => [alpha, 0.0, -alpha],
        };
        self.b = b.map(|v| v / a0);
        self.a = [-2.0 * cos / a0, (1.0 - alpha) / a0];
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// A saw oscillator without aliasing (polyBLEP).
#[derive(Clone, Copy)]
struct Saw {
    phase: f32,
}

impl Saw {
    fn next(&mut self, freq: f32) -> f32 {
        let dt = freq / RATE;
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let t = self.phase;
        let blep = if t < dt {
            let t = t / dt;
            t + t - t * t - 1.0
        } else if t > 1.0 - dt {
            let t = (t - 1.0) / dt;
            t * t + t + t + 1.0
        } else {
            0.0
        };
        2.0 * t - 1.0 - blep
    }
}

fn midi(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

fn sample(seconds: f32) -> usize {
    (seconds * RATE) as usize
}

/// The stereo mix.
struct Mix {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Mix {
    fn add(&mut self, at: usize, value: f32, pan: f32) {
        if at < self.left.len() {
            let angle = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
            self.left[at] += value * angle.cos();
            self.right[at] += value * angle.sin();
        }
    }

    /// Renders `length` seconds of `voice(t)` from `start` seconds.
    fn voice(&mut self, start: f32, length: f32, pan: f32, mut voice: impl FnMut(f32) -> f32) {
        let first = sample(start);
        for i in 0..sample(length) {
            self.add(first + i, voice(i as f32 / RATE), pan);
        }
    }
}

// ------------------------------------------------------------------ Score

/// Chord of each bar (4 beats): root and the notes of the pad, as MIDI notes.
fn chord(beat: f32) -> (f32, [f32; 4]) {
    const AM: (f32, [f32; 4]) = (45.0, [57.0, 60.0, 64.0, 69.0]);
    const F: (f32, [f32; 4]) = (41.0, [57.0, 60.0, 65.0, 69.0]);
    const C: (f32, [f32; 4]) = (48.0, [55.0, 60.0, 64.0, 67.0]);
    const G: (f32, [f32; 4]) = (43.0, [55.0, 59.0, 62.0, 67.0]);
    match (beat / 4.0) as usize {
        1 | 5 => F,
        2 => C,
        3 | 6 => G,
        // bars 0, 4 and from the outro on
        _ => AM,
    }
}

/// A minor pentatonic from A4, for plucks that always fit.
fn pentatonic(step: usize) -> f32 {
    const SCALE: [f32; 5] = [0.0, 3.0, 5.0, 7.0, 10.0];
    69.0 + SCALE[step % 5] + 12.0 * (step / 5) as f32
}

/// Beats with a kick drum.
fn kick_beats() -> Vec<f32> {
    let groove = (0..24).map(|b| b as f32);
    let outro = (28..34).map(|b| b as f32);
    groove.chain(outro).collect()
}

fn kick(mix: &mut Mix, at: f32, gain: f32) {
    mix.voice(at, 0.5, 0.0, |t| {
        // the integral of a pitch that falls from 155 Hz to 45 Hz
        let phase = TAU * (45.0 * t + 110.0 / 32.0 * (1.0 - (-t * 32.0).exp()));
        let body = phase.sin() * (-t * 7.0).exp();
        let click = if t < 0.004 {
            (1.0 - t / 0.004) * 0.6
        } else {
            0.0
        };
        (body + click).tanh() * gain
    });
}

fn clap(mix: &mut Mix, noise: &mut Noise, at: f32, gain: f32) {
    let mut band = Biquad::new(Kind::Band, 1400.0, 1.2);
    let mut values = Vec::with_capacity(sample(0.3));
    for i in 0..sample(0.3) {
        let t = i as f32 / RATE;
        // three quick bursts, then the tail
        let bursts = [0.0, 0.011, 0.022]
            .iter()
            .map(|s| {
                if t >= *s {
                    (-(t - s) * 180.0).exp()
                } else {
                    0.0
                }
            })
            .sum::<f32>();
        let env = bursts * 0.5 + (-t * 18.0).exp() * 0.6;
        values.push(band.run(noise.next()) * env * gain);
    }
    for (i, v) in values.iter().enumerate() {
        mix.add(sample(at) + i, *v, 0.1);
    }
}

fn hat(mix: &mut Mix, noise: &mut Noise, at: f32, gain: f32, pan: f32) {
    let mut high = Biquad::new(Kind::High, 8000.0, 0.8);
    let values: Vec<f32> = (0..sample(0.06))
        .map(|i| high.run(noise.next()) * (-(i as f32 / RATE) * 70.0).exp() * gain)
        .collect();
    for (i, v) in values.iter().enumerate() {
        mix.add(sample(at) + i, *v, pan);
    }
}

/// A boom with a burst of noise for the big cuts.
fn impact(mix: &mut Mix, noise: &mut Noise, at: f32, gain: f32) {
    mix.voice(at, 2.5, 0.0, |t| {
        let phase = TAU * (38.0 * t + 70.0 / 9.0 * (1.0 - (-t * 9.0).exp()));
        (phase.sin() * (-t * 2.2).exp() * 1.2).tanh() * gain
    });
    let mut low = Biquad::new(Kind::Low, 3000.0, 0.7);
    let values: Vec<f32> = (0..sample(1.5))
        .map(|i| {
            let t = i as f32 / RATE;
            low.set(Kind::Low, 300.0 + 5000.0 * (-t * 4.0).exp(), 0.7);
            low.run(noise.next()) * (-t * 3.0).exp() * gain * 0.5
        })
        .collect();
    for (i, v) in values.iter().enumerate() {
        mix.add(sample(at) + i, *v, if i % 2 == 0 { -0.3 } else { 0.3 });
    }
}

/// Noise that sweeps up into a cut.
fn whoosh(mix: &mut Mix, noise: &mut Noise, end: f32, length: f32, gain: f32) {
    let mut band = Biquad::new(Kind::Band, 400.0, 1.5);
    let values: Vec<f32> = (0..sample(length))
        .map(|i| {
            let p = i as f32 / sample(length) as f32;
            band.set(Kind::Band, 300.0 * 20f32.powf(p), 1.5);
            band.run(noise.next()) * p * p * gain
        })
        .collect();
    let start = sample(end - length);
    for (i, v) in values.iter().enumerate() {
        let pan = ((i as f32 / RATE) * 3.0).sin() * 0.6;
        mix.add(start + i, *v, pan);
    }
}

/// A plucked note: two detuned saws through a closing filter.
fn pluck(mix: &mut Mix, at: f32, note: f32, gain: f32, pan: f32) {
    let (mut a, mut b) = (Saw { phase: 0.0 }, Saw { phase: 0.37 });
    let mut low = Biquad::new(Kind::Low, 4000.0, 2.0);
    let freq = midi(note);
    mix.voice(at, 0.8, pan, |t| {
        low.set(Kind::Low, 400.0 + 5000.0 * (-t * 14.0).exp(), 2.0);
        let x = a.next(freq) + b.next(freq * 1.006);
        low.run(x) * (-t * 5.0).exp() * gain
    });
}

/// A key of a mechanical keyboard.
fn key(mix: &mut Mix, noise: &mut Noise, at: f32, gain: f32, pan: f32) {
    let mut band = Biquad::new(Kind::Band, 3500.0, 2.0);
    let values: Vec<f32> = (0..sample(0.025))
        .map(|i| band.run(noise.next()) * (-(i as f32 / RATE) * 250.0).exp() * gain)
        .collect();
    for (i, v) in values.iter().enumerate() {
        mix.add(sample(at) + i, *v, pan);
    }
}

/// Gain of the pad and bass at `t`: they duck under every kick.
fn sidechain(t: f32, kicks: &[f32]) -> f32 {
    let last = kicks
        .iter()
        .map(|b| beat_time(*b))
        .filter(|k| *k <= t)
        .fold(f32::NEG_INFINITY, f32::max);
    if last.is_finite() {
        1.0 - 0.7 * (-(t - last) * 9.0).exp()
    } else {
        1.0
    }
}

/// The whole track, stereo, with its peak at -1 dBFS.
pub fn music() -> PreloadedAudioData<'static> {
    let total = beat_time(END);
    let n = sample(total);
    let mut mix = Mix {
        left: vec![0.0; n],
        right: vec![0.0; n],
    };
    let mut noise = Noise(0x9e37_79b9);
    let kicks = kick_beats();

    // drums
    for b in &kicks {
        kick(&mut mix, beat_time(*b), 0.9);
    }
    for b in (8..24).chain(29..34).filter(|b| b % 2 == 1) {
        clap(&mut mix, &mut noise, beat_time(b as f32), 0.55);
    }
    for i in 0..(END * 4.0) as usize {
        let b = i as f32 / 4.0;
        let offbeat = (b.fract() - 0.5).abs() < 0.01;
        let sixteenth = (16.0..24.0).contains(&b) && b.fract() != 0.0;
        let on = ((4.0..24.0).contains(&b) || (28.0..34.0).contains(&b)) && (offbeat || sixteenth);
        if on {
            let gain = if offbeat { 0.22 } else { 0.08 };
            hat(
                &mut mix,
                &mut noise,
                beat_time(b),
                gain,
                if offbeat { 0.25 } else { -0.25 },
            );
        }
    }
    // the build under the zoom: a snare roll that gets faster
    let mut b = ZOOM;
    while b < OUTRO - 0.1 {
        let progress = (b - ZOOM) / (OUTRO - ZOOM);
        clap(&mut mix, &mut noise, beat_time(b), 0.15 + 0.35 * progress);
        b += if progress < 0.5 {
            0.5
        } else if progress < 0.75 {
            0.25
        } else {
            0.125
        };
    }

    // bass on the offbeats, pad on every beat, both pumping with the kick
    let (mut bass, mut bass2) = (Saw { phase: 0.0 }, Saw { phase: 0.5 });
    let mut bass_filter = Biquad::new(Kind::Low, 500.0, 1.0);
    let mut pad: Vec<[Saw; 2]> = (0..4)
        .map(|i| {
            [
                Saw {
                    phase: i as f32 * 0.21,
                },
                Saw {
                    phase: i as f32 * 0.13 + 0.5,
                },
            ]
        })
        .collect();
    let mut pad_filter = [
        Biquad::new(Kind::Low, 2000.0, 0.7),
        Biquad::new(Kind::Low, 2000.0, 0.7),
    ];
    for i in 0..n {
        let t = i as f32 / RATE;
        let b = t / BEAT;
        let (root, notes) = chord(b);
        let duck = sidechain(t, &kicks);
        // a short gap of silence right before the outro hit
        let silent = (OUTRO - 0.05..OUTRO).contains(&b);

        // bass: eighth notes on the offbeats, silent under the zoom
        let eighth = (b * 2.0).fract();
        let offbeat_note = ((b * 2.0) as usize) % 2 == 1;
        let bass_env = if offbeat_note {
            (-eighth * 2.5).exp()
        } else {
            0.0
        };
        let bass_on = b < ZOOM || (OUTRO..END - 2.0).contains(&b);
        let f = midi(root);
        let bass_value = bass.next(f) + bass2.next(f * 2.0) * 0.4;
        let bass_value = bass_filter.run(bass_value) * bass_env * if bass_on { 0.32 } else { 0.0 };

        // pad: rises with a filter sweep under the zoom
        let sweep = if (ZOOM..OUTRO).contains(&b) {
            (b - ZOOM) / (OUTRO - ZOOM)
        } else {
            0.0
        };
        let cutoff = if b < GRID { 1400.0 } else { 2200.0 } + 5000.0 * sweep * sweep;
        for filter in &mut pad_filter {
            filter.set(Kind::Low, cutoff, 0.9);
        }
        let pad_gain = match b {
            b if b < GRID => 0.05,
            b if b < ZOOM => 0.07,
            b if b < OUTRO => 0.05 + 0.08 * sweep,
            b if b < END - 2.0 => 0.08,
            _ => 0.0,
        };
        let (mut l, mut r) = (0.0, 0.0);
        for (voice, note) in pad.iter_mut().zip(notes) {
            let freq = midi(note);
            l += voice[0].next(freq * 0.997);
            r += voice[1].next(freq * 1.003);
        }
        let l = pad_filter[0].run(l) * pad_gain * duck;
        let r = pad_filter[1].run(r) * pad_gain * duck;

        if !silent {
            mix.left[i] += l + bass_value * duck;
            mix.right[i] += r + bass_value * duck;
        }
    }

    // the hook: hit, word thumps, a rising note per letter
    impact(&mut mix, &mut noise, beat_time(HOOK), 0.8);
    // "NOW PLAYS" gets one hit, because its two words are only 50 ms apart
    kick(&mut mix, beat_time(WORD_NOW), 0.5);
    pluck(&mut mix, beat_time(WORD_NOW), 57.0, 0.25, 0.0);
    for (i, note) in [69.0, 72.0, 74.0, 76.0, 79.0, 81.0].iter().enumerate() {
        pluck(
            &mut mix,
            beat_time(LETTERS + i as f32 * LETTER_STEP),
            *note,
            0.18,
            -0.5 + i as f32 * 0.2,
        );
    }

    // transitions
    for cut in [GRID, CODE] {
        whoosh(&mut mix, &mut noise, beat_time(cut), 0.6, 0.35);
    }
    whoosh(
        &mut mix,
        &mut noise,
        beat_time(OUTRO),
        beat_time(OUTRO - ZOOM),
        0.3,
    );
    impact(&mut mix, &mut noise, beat_time(OUTRO), 1.0);
    impact(&mut mix, &mut noise, beat_time(END - 2.0), 0.7);

    // a pluck for every tile of the grid, panned by its column
    for i in 0..TILES {
        let at = beat_time(TILE_POP + i as f32 * TILE_STEP);
        pluck(
            &mut mix,
            at,
            pentatonic(i + 2),
            0.16,
            ((i % 3) as f32 - 1.0) * 0.6,
        );
    }
    // soft arpeggio under the code
    for i in 0..((ZOOM - CODE) * 2.0) as usize {
        let b = CODE + i as f32 * 0.5;
        let (_, notes) = chord(b);
        pluck(
            &mut mix,
            beat_time(b),
            notes[i % 4] + 12.0,
            0.05,
            if i % 2 == 0 { -0.4 } else { 0.4 },
        );
    }
    // keystrokes, every third one
    for (i, b) in key_beats().iter().enumerate().filter(|(i, _)| i % 3 == 0) {
        key(
            &mut mix,
            &mut noise,
            beat_time(*b),
            0.25,
            if i % 4 == 0 { -0.2 } else { 0.2 },
        );
    }
    // "VECTOR."
    kick(&mut mix, beat_time(ZOOM + 0.5), 0.6);
    // the outro: the last chord rings out
    for (i, note) in [57.0, 60.0, 64.0, 69.0, 72.0].iter().enumerate() {
        pluck(
            &mut mix,
            beat_time(END - 2.0) + i as f32 * 0.02,
            *note,
            0.12,
            -0.4 + i as f32 * 0.2,
        );
    }

    // fade out the last 0.3 s, then normalize the peak to -1 dBFS
    let fade = sample(0.3);
    for i in 0..fade {
        let g = 1.0 - i as f32 / fade as f32;
        mix.left[n - fade + i] *= g;
        mix.right[n - fade + i] *= g;
    }
    let peak = mix
        .left
        .iter()
        .chain(&mix.right)
        .fold(0.0f32, |p, v| p.max(v.abs()));
    let gain = 10f32.powf(-1.0 / 20.0) / peak;

    for v in mix.left.iter_mut().chain(&mut mix.right) {
        *v *= gain;
    }
    PreloadedAudioData {
        samples: Cow::Owned(mix.left),
        sample_rate: RATE as u32,
        right: Some(Cow::Owned(mix.right)),
    }
}
