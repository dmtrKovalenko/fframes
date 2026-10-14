//! Per-track effects: loudness normalization, RBJ biquad filters, a compressor and stereo
//! width. See `AudioTrack::highpass` and the other builder methods.
//!
//! The mixer renders a track with effects once, when the mixer is built. It reads the part of
//! the file that plays at the mix rate (resampled, at the track's `rate`). Then it runs the
//! effects in the order of the calls. Gain, pan, fades and ducking apply after the effects.
use crate::audio_analysis::{Biquad, LoudnessAnalysis};
use serde::Serialize;
use std::f64::consts::TAU;
use std::fmt;

/// One processing step of a track, see the `AudioTrack` methods with the same names.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Gain that brings the integrated loudness of the track to `lufs`.
    NormalizeLufs {
        lufs: f32,
    },
    /// 12 dB per octave below `hz`. `q` 0.707 is Butterworth (-3 dB at `hz`).
    Highpass {
        hz: f32,
        q: f32,
    },
    /// 12 dB per octave above `hz`. `q` 0.707 is Butterworth (-3 dB at `hz`).
    Lowpass {
        hz: f32,
        q: f32,
    },
    /// `db` below `hz`, half of `db` at `hz`.
    LowShelf {
        hz: f32,
        db: f32,
    },
    /// `db` above `hz`, half of `db` at `hz`.
    HighShelf {
        hz: f32,
        db: f32,
    },
    /// Bell: `db` at `hz`, bandwidth `hz / q`.
    Peak {
        hz: f32,
        db: f32,
        q: f32,
    },
    Compress(Compressor),
    /// Mid/side width: 0 mono, 1 unchanged, above 1 wider.
    Width {
        width: f32,
    },
}

/// Feed-forward compressor with a peak detector, linked on both channels.
///
/// The input level is the larger absolute sample of the two channels. The static curve turns
/// this level into a gain reduction in dB. A smooth decoupled peak detector (Giannoulis,
/// Massberg and Reiss, 2012) follows the reduction: it holds the peaks with `release`, and
/// then smooths the result with `attack`. Thus the compressor reacts to peaks, also on short
/// hits, and a steady tone gets the gain of the static curve at its peak level.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Compressor {
    /// Level in dBFS above which the compressor reduces the gain. Default `-18.0`.
    pub threshold_db: f32,
    /// Input dB above the threshold for each output dB. `1.0` does nothing, `20.0` and more
    /// is a limiter. Default `3.0`.
    pub ratio: f32,
    /// Time constant in seconds of the gain going down (63% of a step). Default `0.01`.
    pub attack: f32,
    /// Time constant in seconds of the gain coming back. Default `0.15`.
    pub release: f32,
    /// Width in dB of the soft knee around the threshold, `0.0` is a hard knee. Default `6.0`.
    pub knee_db: f32,
    /// Gain in dB after the compression. Default `0.0`.
    pub makeup_db: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        Self {
            threshold_db: -18.,
            ratio: 3.,
            attack: 0.01,
            release: 0.15,
            knee_db: 6.,
            makeup_db: 0.,
        }
    }
}

impl Compressor {
    /// Output level in dB for a steady input level in dB.
    pub fn static_curve(&self, input_db: f32) -> f32 {
        let over = input_db - self.threshold_db;
        let slope = 1. / self.ratio.max(1.) - 1.;
        let knee = self.knee_db.max(0.);
        if 2. * over <= -knee {
            input_db
        } else if 2. * over.abs() < knee {
            input_db + slope * (over + knee / 2.).powi(2) / (2. * knee)
        } else {
            input_db + slope * over
        }
    }

    fn process(&self, channels: &mut [Vec<f32>], sample_rate: f64) {
        let coefficient = |seconds: f32| (-1. / (f64::from(seconds).max(1e-5) * sample_rate)).exp();
        let (attack, release) = (coefficient(self.attack), coefficient(self.release));
        let makeup = f64::from(self.makeup_db);
        let (mut held, mut reduction) = (0f64, 0.);

        for i in 0..channels[0].len() {
            let peak = channels.iter().fold(0f32, |m, c| m.max(c[i].abs()));
            let level = 20. * peak.max(1e-9).log10();
            let target = f64::from(level - self.static_curve(level));
            held = target.max(release * held + (1. - release) * target);
            reduction = attack * reduction + (1. - attack) * held;
            let gain = db_to_gain(makeup - reduction);
            for channel in channels.iter_mut() {
                channel[i] = (f64::from(channel[i]) * gain) as f32;
            }
        }
    }
}

fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.)
}

/// RBJ Audio EQ Cookbook filters. Frequencies stay below Nyquist.
fn biquad(effect: &Effect, sample_rate: f64) -> Option<Biquad> {
    let omega = |hz: f32| TAU * f64::from(hz).clamp(1., sample_rate * 0.49) / sample_rate;
    let alpha = |w: f64, q: f32| w.sin() / (2. * f64::from(q).max(0.05));
    Some(match *effect {
        Effect::Highpass { hz, q } => {
            let w = omega(hz);
            let (cos, alpha) = (w.cos(), alpha(w, q));
            let b = f64::midpoint(1., cos);
            Biquad::new([b, -2. * b, b], [1. + alpha, -2. * cos, 1. - alpha])
        }
        Effect::Lowpass { hz, q } => {
            let w = omega(hz);
            let (cos, alpha) = (w.cos(), alpha(w, q));
            let b = f64::midpoint(1., -cos);
            Biquad::new([b, 2. * b, b], [1. + alpha, -2. * cos, 1. - alpha])
        }
        Effect::Peak { hz, db, q } => {
            let w = omega(hz);
            let (cos, alpha) = (w.cos(), alpha(w, q));
            let a = 10f64.powf(f64::from(db) / 40.);
            Biquad::new(
                [1. + alpha * a, -2. * cos, 1. - alpha * a],
                [1. + alpha / a, -2. * cos, 1. - alpha / a],
            )
        }
        Effect::LowShelf { hz, db } | Effect::HighShelf { hz, db } => {
            let w = omega(hz);
            let a = 10f64.powf(f64::from(db) / 40.);
            // Shelf slope S = 1: the steepest slope without a bump.
            let (cos, k) = (w.cos(), a.sqrt() * w.sin() * std::f64::consts::SQRT_2);
            // A high shelf is the low shelf with the sign of `cos` swapped.
            let (sign, cos) = match effect {
                Effect::LowShelf { .. } => (1., cos),
                _ => (-1., -cos),
            };
            Biquad::new(
                [
                    a * ((a + 1.) - (a - 1.) * cos + k),
                    sign * 2. * a * ((a - 1.) - (a + 1.) * cos),
                    a * ((a + 1.) - (a - 1.) * cos - k),
                ],
                [
                    (a + 1.) + (a - 1.) * cos + k,
                    sign * -2. * ((a - 1.) + (a + 1.) * cos),
                    (a + 1.) + (a - 1.) * cos - k,
                ],
            )
        }
        Effect::NormalizeLufs { .. } | Effect::Compress(_) | Effect::Width { .. } => return None,
    })
}

/// Seconds of delay of the side signal made for a mono track.
const MONO_SIDE_DELAY: f64 = 0.012;
/// Below this frequency the side signal made for a mono track is cut, so the bass stays mono.
const MONO_SIDE_HIGHPASS: f32 = 250.;
/// Level of the side signal made for a mono track, for each unit of width.
const MONO_SIDE_LEVEL: f32 = 0.3;

fn width(channels: &mut Vec<Vec<f32>>, width: f32, sample_rate: f64) {
    let width = width.max(0.);
    if let [left, right] = channels.as_mut_slice() {
        if width == 1. {
            return;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mid = f32::midpoint(*l, *r);
            let side = (*l - *r) * 0.5 * width;
            (*l, *r) = (mid + side, mid - side);
        }
    } else if width > 0. {
        // The side is the mono signal, delayed and high-passed. L = M + S and R = M - S, so
        // the mono sum L + R stays 2M at every frequency.
        let mid = &channels[0];
        let delay = (MONO_SIDE_DELAY * sample_rate).round() as usize;
        let mut highpass = biquad(
            &Effect::Highpass {
                hz: MONO_SIDE_HIGHPASS,
                q: std::f32::consts::FRAC_1_SQRT_2,
            },
            sample_rate,
        )
        .expect("a highpass is a biquad");
        let level = (MONO_SIDE_LEVEL * width).min(1.);
        let side: Vec<f32> = (0..mid.len())
            .map(|i| {
                let delayed = i.checked_sub(delay).map_or(0., |j| mid[j]);
                highpass.process(f64::from(delayed)) as f32 * level
            })
            .collect();
        let right = mid.iter().zip(&side).map(|(m, s)| m - s).collect();
        channels[0].iter_mut().zip(&side).for_each(|(m, s)| *m += s);
        channels.push(right);
    }
}

/// Runs `effects` in order on one or two channels at `sample_rate`. A mono track becomes
/// stereo after `Width`. Returns the sum of the gains in dB that `NormalizeLufs` applied.
pub(crate) fn apply_effects(
    effects: &[Effect],
    sample_rate: usize,
    channels: &mut Vec<Vec<f32>>,
) -> f32 {
    let rate = sample_rate as f64;
    let mut normalize_db = 0.;
    for effect in effects {
        match effect {
            Effect::NormalizeLufs { lufs } => {
                let (left, right) = (&channels[0], &channels[channels.len() - 1]);
                // A mono track plays on both sides, so it is measured on both sides.
                let measured =
                    LoudnessAnalysis::new(left, right, sample_rate).integrated(0..left.len());
                // Silence and tracks shorter than one 400 ms block have no loudness.
                if measured.is_finite() {
                    let db = f64::from(*lufs) - measured;
                    let gain = db_to_gain(db) as f32;
                    channels.iter_mut().flatten().for_each(|s| *s *= gain);
                    normalize_db += db as f32;
                }
            }
            Effect::Compress(compressor) => compressor.process(channels, rate),
            Effect::Width { width: w } => width(channels, *w, rate),
            filter => {
                if let Some(filter) = biquad(filter, rate) {
                    for channel in channels.iter_mut() {
                        let mut filter = filter;
                        for s in channel {
                            *s = filter.process(f64::from(*s)) as f32;
                        }
                    }
                }
            }
        }
    }
    normalize_db
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::NormalizeLufs { lufs } => write!(f, "normalize {lufs:.1} LUFS"),
            Effect::Highpass { hz, q } => write!(f, "highpass {hz:.0} Hz Q {q:.2}"),
            Effect::Lowpass { hz, q } => write!(f, "lowpass {hz:.0} Hz Q {q:.2}"),
            Effect::LowShelf { hz, db } => write!(f, "low shelf {hz:.0} Hz {db:+.1} dB"),
            Effect::HighShelf { hz, db } => write!(f, "high shelf {hz:.0} Hz {db:+.1} dB"),
            Effect::Peak { hz, db, q } => write!(f, "peak {hz:.0} Hz {db:+.1} dB Q {q:.2}"),
            Effect::Compress(c) => write!(
                f,
                "compress {:.1} dB {:.1}:1 {:.0}/{:.0} ms",
                c.threshold_db,
                c.ratio,
                c.attack * 1000.,
                c.release * 1000.
            ),
            Effect::Width { width } => write!(f, "width {width:.2}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: usize = 48000;

    fn sine(hz: f64, amplitude: f32, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE as f64) as usize)
            .map(|i| amplitude * (TAU * hz * i as f64 / RATE as f64).sin() as f32)
            .collect()
    }

    fn rms_db(samples: &[f32]) -> f64 {
        let mean =
            samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64;
        10. * mean.log10()
    }

    /// Gain in dB of `effect` for a sine of `hz`, measured after the filter settles.
    fn response_db(effect: Effect, hz: f64) -> f64 {
        let mut channels = vec![sine(hz, 0.5, 1.)];
        apply_effects(&[effect], RATE, &mut channels);
        let tail = RATE / 2..RATE;
        rms_db(&channels[0][tail.clone()]) - rms_db(&sine(hz, 0.5, 1.)[tail])
    }

    #[track_caller]
    fn assert_near(value: f64, expected: f64, tolerance: f64) {
        assert!(
            (value - expected).abs() <= tolerance,
            "{value:.3} is not {expected} ± {tolerance}"
        );
    }

    #[test]
    fn highpass_and_lowpass_are_butterworth() {
        let highpass = Effect::Highpass {
            hz: 200.,
            q: std::f32::consts::FRAC_1_SQRT_2,
        };
        assert_near(response_db(highpass, 200.), -3.01, 0.1);
        assert!(response_db(highpass, 30.) < -20.);
        assert_near(response_db(highpass, 2000.), 0., 0.05);

        let lowpass = Effect::Lowpass {
            hz: 2000.,
            q: std::f32::consts::FRAC_1_SQRT_2,
        };
        assert_near(response_db(lowpass, 2000.), -3.01, 0.1);
        assert!(response_db(lowpass, 16000.) < -30.);
        assert_near(response_db(lowpass, 100.), 0., 0.05);
    }

    #[test]
    fn shelves_and_peak_reach_their_gain() {
        let low = Effect::LowShelf { hz: 200., db: 6. };
        assert_near(response_db(low, 20.), 6., 0.2);
        assert_near(response_db(low, 200.), 3., 0.1);
        assert_near(response_db(low, 8000.), 0., 0.1);

        let high = Effect::HighShelf { hz: 4000., db: -9. };
        assert_near(response_db(high, 18000.), -9., 0.3);
        assert_near(response_db(high, 4000.), -4.5, 0.1);
        assert_near(response_db(high, 100.), 0., 0.1);

        let peak = Effect::Peak {
            hz: 1000.,
            db: 5.,
            q: 2.,
        };
        assert_near(response_db(peak, 1000.), 5., 0.05);
        assert_near(response_db(peak, 100.), 0., 0.1);
        assert_near(response_db(peak, 10000.), 0., 0.1);
    }

    #[test]
    fn compressor_follows_its_static_curve() {
        let compressor = Compressor {
            threshold_db: -20.,
            ratio: 4.,
            attack: 0.005,
            release: 0.1,
            knee_db: 6.,
            makeup_db: 0.,
        };
        // A sine with its peaks at -30 dBFS for 1 s, then at 0 dBFS for 1 s.
        let quiet = sine(1000., 10f32.powf(-30. / 20.), 1.);
        let mut input = quiet.clone();
        input.extend(sine(1000., 1., 1.));
        let mut channels = vec![input.clone(), input];
        apply_effects(&[Effect::Compress(compressor)], RATE, &mut channels);

        // Below the threshold and the knee the samples do not change at all.
        assert_eq!(channels[0][..RATE], quiet[..]);
        assert_eq!(channels[0], channels[1]);

        let peak_db = |range: std::ops::Range<usize>| {
            20. * channels[0][range]
                .iter()
                .fold(0f32, |m, s| m.max(s.abs()))
                .log10()
        };
        let expected = compressor.static_curve(0.);
        assert_near(f64::from(expected), -15., 1e-6);
        // Five attack time constants (25 ms) after the step the level is on the curve.
        let ms = RATE / 1000;
        assert_near(
            f64::from(peak_db(RATE + 25 * ms..RATE + 30 * ms)),
            -15.,
            0.5,
        );
        assert_near(f64::from(peak_db(RATE + 500 * ms..2 * RATE)), -15., 0.1);
    }

    #[test]
    fn compressor_releases_after_a_hit() {
        let compressor = Compressor::default();
        let mut input = sine(1000., 1., 0.5);
        input.extend(sine(1000., 0.01, 1.));
        let mut channels = vec![input];
        apply_effects(&[Effect::Compress(compressor)], RATE, &mut channels);
        let peak = |range: std::ops::Range<usize>| {
            channels[0][range].iter().fold(0f32, |m, s| m.max(s.abs()))
        };
        // Five release time constants (750 ms) after the hit the gain is back to unity.
        assert_near(
            f64::from(peak(RATE / 2 + RATE * 3 / 4..RATE * 3 / 2)),
            0.01,
            1e-4,
        );
    }

    #[test]
    fn width_scales_the_side() {
        let left = sine(300., 0.5, 0.5);
        let right = sine(700., 0.4, 0.5);

        let mut mono = vec![left.clone(), right.clone()];
        apply_effects(&[Effect::Width { width: 0. }], RATE, &mut mono);
        assert_eq!(mono[0], mono[1]);

        let mut same = vec![left.clone(), right.clone()];
        apply_effects(&[Effect::Width { width: 1. }], RATE, &mut same);
        assert_eq!(same, [left.clone(), right.clone()]);

        let mut wide = vec![left.clone(), right.clone()];
        apply_effects(&[Effect::Width { width: 1.8 }], RATE, &mut wide);
        let sum =
            |c: &[Vec<f32>]| -> Vec<f32> { c[0].iter().zip(&c[1]).map(|(l, r)| l + r).collect() };
        assert_near(rms_db(&sum(&wide)), rms_db(&sum(&[left, right])), 0.5);
    }

    #[test]
    fn width_on_a_mono_track_makes_it_stereo_and_keeps_the_mono_sum() {
        let tone = sine(1000., 0.5, 0.5);
        let mut channels = vec![tone.clone()];
        apply_effects(&[Effect::Width { width: 1.5 }], RATE, &mut channels);
        assert_eq!(channels.len(), 2);
        assert_ne!(channels[0], channels[1]);
        for i in 0..tone.len() {
            assert!((channels[0][i] + channels[1][i] - 2. * tone[i]).abs() < 1e-6);
        }

        let mut unchanged = vec![tone.clone()];
        apply_effects(&[Effect::Width { width: 0. }], RATE, &mut unchanged);
        assert_eq!(unchanged, [tone]);
    }
}
