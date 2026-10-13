//! Synthesizes the sound of UNIT 01 into `dynamic_media/`: a drone, the air, the snaps and a
//! chime. The video places the sounds at the snap times from `assembly_example::SNAPS`.
//!
//! `cargo run --release -p assembly-example --bin assembly-soundtrack`

use std::f32::consts::{PI, TAU};
use std::path::Path;

use assembly_example::DURATION;
use fframes_3d::math::smoothstep;

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

// ------------------------------------------------------------------ Sounds

/// A soft, deep drone: two detuned sines and filtered noise. Its level rises and falls very
/// slowly.
fn drone() -> Track {
    let length = DURATION + 1.0;
    let mut track = Track::new(length);
    for (channel, detune) in [(0usize, 0.997f32), (1, 1.003)] {
        let mut a = Sine::default();
        let mut b = Sine::default();
        let mut noise = Noise(5 + channel as u32);
        let mut low = Biquad::new(Kind::Low, 400.0, 0.7);
        let s = signal(length, |t| {
            let breath = 0.75 + 0.25 * (t * 0.6).sin();
            let tone = a.next(55.0 * detune) * 0.6 + b.next(82.4 * detune) * 0.25;
            let air = low.run(noise.next()) * 0.25;
            (tone + air) * breath * smoothstep(0.0, 2.0, t)
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

/// The air that a part pushes on its way in: filtered noise that swells into the snap.
fn air() -> Track {
    let length = 0.6;
    let mut noise = Noise(9);
    let mut band = Biquad::new(Kind::Band, 500.0, 0.8);
    let s = signal(length, |t| {
        let x = t / length;
        band.set(Kind::Band, 300.0 + 2500.0 * x * x, 0.9);
        band.run(noise.next()) * x * x
    });
    mono(&s, -3.0)
}

/// A precise snap: a short click, a damped body and a little sub bass.
fn snap(pitch: f32, seed: u32) -> Track {
    let mut noise = Noise(seed);
    let mut high = Biquad::new(Kind::High, 3000.0, 0.7);
    let mut body = Biquad::new(Kind::Band, 900.0 * pitch, 6.0);
    let mut sub = Sine::default();
    let s = signal(0.5, |t| {
        let n = noise.next();
        let click = high.run(n) * decay(t, 600.0);
        let thock = body.run(n * decay(t, 200.0)) * 6.0 * decay(t, 25.0);
        let low = sub.next(70.0 * pitch + 50.0 * decay(t, 50.0)) * decay(t, 14.0) * 0.8;
        click * 0.8 + thock + low
    });
    let mut track = reverb(&mono(&s, -1.0), 0.7, 0.5, 0.12);
    track.normalize(-1.0);
    track
}

/// A soft bell with three partials for the visor light.
fn chime() -> Track {
    let mut a = Sine::default();
    let mut b = Sine::default();
    let mut c = Sine::default();
    let s = signal(3.0, |t| {
        (a.next(880.0) * 0.6
            + b.next(1318.5) * 0.35 * decay(t, 1.5)
            + c.next(2637.0) * 0.12 * decay(t, 4.0))
            * attack_decay(t, 0.004, 1.3)
    });
    let mut track = reverb(&mono(&s, -3.0), 0.88, 0.3, 0.4);
    track.normalize(-2.0);
    track
}

fn main() -> std::io::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    std::fs::create_dir_all(&dir)?;
    for (name, track) in [
        ("drone.wav", drone()),
        ("air.wav", air()),
        ("snap1.wav", snap(1.0, 11)),
        ("snap2.wav", snap(1.12, 23)),
        ("snap_head.wav", snap(0.8, 37)),
        ("chime.wav", chime()),
    ] {
        track.write(&dir.join(name))?;
        println!(
            "wrote dynamic_media/{name} ({:.2} s)",
            track.len() as f32 / RATE
        );
    }
    Ok(())
}
