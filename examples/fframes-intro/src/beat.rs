//! The musical clock. The soundtrack is an edit of a 132 BPM track whose
//! cuts all land on phrase boundaries, so the whole video sits on one beat
//! grid: every scene starts on a downbeat and animations are written in beats.

use fframes::Frame;

pub const FPS: usize = 60;
/// One beat of the soundtrack, in seconds (132.007 BPM, measured).
pub const BEAT: f32 = 0.454_522;
/// First downbeat of the soundtrack (bar 1, beat 1).
pub const DOWNBEAT0: f32 = 2.148_51;
/// The music ends with a decay; the video stops with it.
pub const TOTAL_SECONDS: f32 = 127.5;

pub fn beat_time(beat: f32) -> f32 {
    DOWNBEAT0 + beat * BEAT
}

/// The frame a beat starts on, rounded so scene boundaries never drift.
pub fn beat_frame(beat: f32) -> usize {
    (beat_time(beat) * FPS as f32).round() as usize
}

/// Seconds since the start of the video, whichever scene the frame belongs to.
pub fn gsec(frame: &Frame) -> f32 {
    frame.global_index as f32 / FPS as f32
}

/// Position on the beat grid of the whole video (negative before bar 1).
pub fn gbeat(frame: &Frame) -> f32 {
    (gsec(frame) - DOWNBEAT0) / BEAT
}

/// Number of frames between two beats; `None` stands for the start or end of
/// the video.
pub fn span_frames(start: Option<f32>, end: Option<f32>) -> usize {
    let a = start.map(beat_frame).unwrap_or(0);
    let b = end
        .map(beat_frame)
        .unwrap_or((TOTAL_SECONDS * FPS as f32).round() as usize);
    b - a
}

// ---------------------------------------------------------------------------
// easing, all on 0..1

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Where `x` is between `a` and `b`, clamped to 0..1.
pub fn prog(x: f32, a: f32, b: f32) -> f32 {
    clamp01((x - a) / (b - a))
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn expo_out(t: f32) -> f32 {
    let t = clamp01(t);
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2f32.powf(-10.0 * t)
    }
}

pub fn expo_in(t: f32) -> f32 {
    let t = clamp01(t);
    if t <= 0.0 {
        0.0
    } else {
        2f32.powf(10.0 * t - 10.0)
    }
}

pub fn cubic_in_out(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

pub fn quart_out(t: f32) -> f32 {
    1.0 - (1.0 - clamp01(t)).powi(4)
}

/// Damped spring from 0 to 1 (mass 1), `t` in seconds since release.
pub fn spring(t: f32, stiffness: f32, damping: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    let w0 = stiffness.sqrt();
    let zeta = damping / (2.0 * w0);
    if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        1.0 - (-zeta * w0 * t).exp() * ((wd * t).cos() + zeta * w0 / wd * (wd * t).sin())
    } else {
        1.0 - (1.0 + w0 * t) * (-w0 * t).exp()
    }
}

/// A snappy UI spring, `beats` since it was released.
pub fn snap(beats: f32) -> f32 {
    spring(beats * BEAT, 320.0, 22.0)
}

/// A softer spring with a visible overshoot.
pub fn soft(beats: f32) -> f32 {
    spring(beats * BEAT, 170.0, 15.0)
}

/// Decaying pulse that fires on every beat: 1 on the beat, ~0 half a beat later.
pub fn pulse(beat: f32, sharpness: f32) -> f32 {
    let f = beat - beat.floor();
    (-f * sharpness).exp()
}

/// Deterministic hash noise, 0..1.
pub fn hash(n: f32) -> f32 {
    let x = (n * 127.1 + 311.7).sin() * 43_758.547;
    x - x.floor()
}
