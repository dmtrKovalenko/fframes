//! The audio mixer: places every track of the resolved audio map on the output timeline
//! with sample accuracy and mixes them to stereo.
//!
//! - tracks start and end at their exact sample, also in the middle of an encoder frame,
//! - each track has gain, pan, fades, a file offset and range-driven ducking (`TrackMix`),
//! - a Kaiser-windowed sinc resamples sources at other sample rates,
//! - the mixer sums overlapping tracks linearly and runs the sum through a lookahead peak
//!   limiter, so loud overlaps neither intermodulate nor clip.
use crate::{
    AudioData, AudioTimelineSamples, AudioTimelineUnit, Ducking, MediaProvider, ResolvedAudioMap,
    TrackMix,
};
use std::collections::VecDeque;
use std::ops::Range;

/// Master bus settings.
#[derive(Debug, Clone, Copy)]
pub struct AudioMixOptions {
    pub master_gain_db: f32,
    /// Peak limiter on the master bus. `None` sums without any protection against clipping.
    pub limiter: Option<LimiterOptions>,
    /// 5 ms fades wherever a track or the rendered range cuts into a sound, so cuts do not
    /// click.
    pub declick: bool,
}

impl Default for AudioMixOptions {
    fn default() -> Self {
        Self {
            master_gain_db: 0.,
            limiter: Some(LimiterOptions::default()),
            declick: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LimiterOptions {
    /// Maximum sample level in dBFS.
    pub ceiling_db: f32,
    /// The limiter starts reducing the level this long before a peak.
    pub lookahead_ms: f32,
    /// Time constant of the level coming back after a peak.
    pub release_ms: f32,
}

impl Default for LimiterOptions {
    fn default() -> Self {
        Self {
            ceiling_db: -1.,
            lookahead_ms: 5.,
            release_ms: 80.,
        }
    }
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.)
}

const DECLICK_SECONDS: f64 = 0.005;

/// Modified Bessel function of the first kind, order 0 (for the Kaiser window).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.;
    let mut term = 1.;
    let half = x / 2.;
    for k in 1..50 {
        term *= (half / f64::from(k)) * (half / f64::from(k));
        sum += term;
        if term < sum * 1e-12 {
            break;
        }
    }
    sum
}

/// Polyphase table of a Kaiser-windowed sinc low-pass. When downsampling the cutoff drops to
/// the output Nyquist so nothing above it aliases.
struct SincResampler {
    half: usize,
    phases: usize,
    table: Vec<f32>,
}

impl SincResampler {
    const ZERO_CROSSINGS: f64 = 8.;
    const PHASES: usize = 256;
    const KAISER_BETA: f64 = 8.6;

    fn new(source_rate: f64, output_rate: f64) -> Self {
        let cutoff = (output_rate / source_rate).min(1.) * 0.95;
        let half = (Self::ZERO_CROSSINGS / cutoff).ceil() as usize;
        let taps = 2 * half;
        let phases = Self::PHASES;
        let norm = bessel_i0(Self::KAISER_BETA);
        let mut table = Vec::with_capacity((phases + 1) * taps);

        for p in 0..=phases {
            let frac = p as f64 / phases as f64;
            let row_start = table.len();
            for j in 0..taps {
                // Distance between the interpolated position and source sample `floor + k`.
                let k = j as f64 - half as f64 + 1.;
                let d = k - frac;
                let v = d / half as f64;
                let window = if v.abs() < 1. {
                    bessel_i0(Self::KAISER_BETA * (1. - v * v).sqrt()) / norm
                } else {
                    0.
                };
                let x = cutoff * d;
                let sinc = if x.abs() < 1e-12 {
                    1.
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                table.push((cutoff * sinc * window) as f32);
            }
            // Unity DC gain for every phase.
            let sum: f32 = table[row_start..].iter().sum();
            if sum.abs() > 1e-9 {
                table[row_start..].iter_mut().for_each(|c| *c /= sum);
            }
        }

        Self {
            half,
            phases,
            table,
        }
    }

    /// Value of `samples` at the fractional position `t`.
    fn sample(&self, samples: &[f32], t: f64) -> f32 {
        let base = t.floor();
        let frac = t - base;
        let base = base as isize;
        let p = frac * self.phases as f64;
        let p0 = (p.floor() as usize).min(self.phases - 1);
        let w = (p - p0 as f64) as f32;
        let taps = 2 * self.half;
        let row0 = &self.table[p0 * taps..(p0 + 1) * taps];
        let row1 = &self.table[(p0 + 1) * taps..(p0 + 2) * taps];

        let mut acc = 0.;
        for j in 0..taps {
            let index = base + j as isize - self.half as isize + 1;
            if index >= 0 && (index as usize) < samples.len() {
                let coef = row0[j] + (row1[j] - row0[j]) * w;
                acc += samples[index as usize] * coef;
            }
        }
        acc
    }
}

/// Lookahead brickwall limiter (linked stereo). The gain is the sliding minimum of the gain
/// each sample needs, released exponentially and smoothed by a moving average as long as the
/// lookahead, so it reaches the required gain exactly at the (delayed) peak.
struct Limiter {
    ceiling: f64,
    lookahead: usize,
    beta: f64,
    delay: VecDeque<(f32, f32)>,
    min_window: VecDeque<(u64, f64)>,
    released: f64,
    boxcar: Vec<f64>,
    boxcar_sum: f64,
    n: u64,
}

impl Limiter {
    fn new(options: LimiterOptions, sample_rate: usize) -> Self {
        let lookahead = ((f64::from(options.lookahead_ms) / 1000. * sample_rate as f64).round()
            as usize)
            .max(1);
        let release = (f64::from(options.release_ms) / 1000.).max(1e-4);
        let mut limiter = Self {
            ceiling: 10f64.powf(f64::from(options.ceiling_db) / 20.),
            lookahead,
            beta: 1. - (-1. / (release * sample_rate as f64)).exp(),
            delay: VecDeque::with_capacity(lookahead),
            min_window: VecDeque::new(),
            released: 1.,
            boxcar: vec![1.; lookahead],
            boxcar_sum: lookahead as f64,
            n: 0,
        };
        limiter.reset();
        limiter
    }

    /// Output samples lag the input by this many samples.
    fn latency(&self) -> usize {
        self.lookahead - 1
    }

    fn reset(&mut self) {
        self.delay.clear();
        for _ in 0..self.latency() {
            self.delay.push_back((0., 0.));
        }
        self.min_window.clear();
        self.released = 1.;
        self.boxcar.iter_mut().for_each(|g| *g = 1.);
        self.boxcar_sum = self.lookahead as f64;
        self.n = 0;
    }

    fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let peak = f64::from(left.abs().max(right.abs()));
        let required = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.
        };

        while self.min_window.back().is_some_and(|(_, g)| *g >= required) {
            self.min_window.pop_back();
        }
        self.min_window.push_back((self.n, required));
        while self
            .min_window
            .front()
            .is_some_and(|(i, _)| *i + self.lookahead as u64 <= self.n)
        {
            self.min_window.pop_front();
        }
        let hold = self.min_window.front().map_or(1., |(_, g)| *g);

        self.released = if hold < self.released {
            hold
        } else {
            self.released + (hold - self.released) * self.beta
        };

        let slot = (self.n % self.lookahead as u64) as usize;
        self.boxcar_sum += self.released - self.boxcar[slot];
        self.boxcar[slot] = self.released;
        if self.n.is_multiple_of(1_000_000) {
            // Kill floating point drift of the running sum.
            self.boxcar_sum = self.boxcar.iter().sum();
        }
        let gain = (self.boxcar_sum / self.lookahead as f64) as f32;
        self.n += 1;

        self.delay.push_back((left, right));
        let (dl, dr) = self.delay.pop_front().unwrap_or((0., 0.));
        let ceiling = self.ceiling as f32;
        (
            (dl * gain).clamp(-ceiling, ceiling),
            (dr * gain).clamp(-ceiling, ceiling),
        )
    }
}

struct PreparedTrack<'m> {
    file: String,
    left: &'m [f32],
    right: Option<&'m [f32]>,
    /// Output samples.
    range: Range<usize>,
    /// Position in the file (source samples) at `range.start`.
    source_start: f64,
    /// Source samples per output sample.
    ratio: f64,
    resampler: Option<SincResampler>,
    gain: f32,
    /// Mono: equal-power pan gains; stereo: balance attenuation per side.
    pan: (f32, f32),
    mix: TrackMix,
    fade_in: f64,
    fade_out: f64,
    declick_in: bool,
    declick_out: bool,
    /// Voice ranges (output samples) this track is ducked under, merged.
    duck_under: Vec<Range<f64>>,
}

impl PreparedTrack<'_> {
    fn source_sample(&self, channel: &[f32], position: f64) -> f32 {
        match &self.resampler {
            None => channel
                .get(position.round() as usize)
                .copied()
                .unwrap_or(0.),
            Some(resampler) => resampler.sample(channel, position),
        }
    }

    fn envelope(&self, n: usize, sample_rate: f64) -> f32 {
        let from_start = (n - self.range.start) as f64;
        let to_end = (self.range.end - n) as f64;
        let mut gain = self.gain;

        if self.fade_in > 0. && from_start < self.fade_in {
            gain *= self.mix.fade_curve.gain((from_start / self.fade_in) as f32);
        } else if self.declick_in && from_start < DECLICK_SECONDS * sample_rate {
            gain *= (from_start / (DECLICK_SECONDS * sample_rate)) as f32;
        }

        if self.fade_out > 0. && to_end <= self.fade_out {
            gain *= self
                .mix
                .fade_curve
                .gain(((to_end - 1.) / self.fade_out) as f32);
        } else if self.declick_out && to_end <= DECLICK_SECONDS * sample_rate {
            gain *= ((to_end - 1.) / (DECLICK_SECONDS * sample_rate)).max(0.) as f32;
        }

        if let Some(ducking) = self.mix.duck {
            gain *= db_to_gain(duck_db(&self.duck_under, &ducking, n as f64, sample_rate));
        }

        gain
    }
}

/// Attenuation of a ducked track at output sample `n`.
fn duck_db(voices: &[Range<f64>], ducking: &Ducking, n: f64, sample_rate: f64) -> f32 {
    let attack = f64::from(ducking.attack) * sample_rate;
    let hold = f64::from(ducking.hold) * sample_rate;
    let release = f64::from(ducking.release) * sample_rate;
    let raised_cosine = |t: f64| (1. - (t.clamp(0., 1.) * std::f64::consts::PI).cos()) * 0.5;

    let mut depth: f64 = 0.;
    for voice in voices {
        let down_from = voice.start - attack;
        let up_from = voice.end + hold;
        if n < down_from || n >= up_from + release {
            continue;
        }
        let amount = if n < voice.start {
            if attack > 0. {
                raised_cosine((n - down_from) / attack)
            } else {
                1.
            }
        } else if n < up_from {
            1.
        } else {
            1. - raised_cosine((n - up_from) / release.max(1.))
        };
        depth = depth.max(amount);
    }
    ducking.depth_db * depth as f32
}

/// Mixes the resolved audio map block by block. Blocks are expected in order (the limiter is
/// stateful); jumping elsewhere resets the limiter.
pub struct AudioMixer<'m> {
    tracks: Vec<PreparedTrack<'m>>,
    sample_rate: usize,
    master_gain: f32,
    limiter: Option<Limiter>,
    /// The next raw sample the limiter expects.
    next_raw: Option<usize>,
    output_range: Range<usize>,
    total_samples: usize,
    declick: bool,
    missing: Vec<String>,
}

impl<'m> AudioMixer<'m> {
    /// `output_range` is the part of the timeline being rendered (in samples at
    /// `sample_rate`), `total_samples` the length of the whole video.
    pub fn new(
        audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
        media: Option<&'m dyn MediaProvider<'m>>,
        sample_rate: usize,
        output_range: Range<usize>,
        total_samples: usize,
        options: AudioMixOptions,
    ) -> Self {
        Self::new_rescaled(
            audio_map,
            sample_rate,
            media,
            sample_rate,
            output_range,
            total_samples,
            options,
        )
    }

    /// Like `new` for an audio map resolved at `map_sample_rate` while the output runs at
    /// `sample_rate`, e.g. when the encoder does not support the requested rate (Opus is
    /// 48 kHz only). `output_range` and `total_samples` are at `sample_rate`.
    pub fn new_rescaled(
        audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
        map_sample_rate: usize,
        media: Option<&'m dyn MediaProvider<'m>>,
        sample_rate: usize,
        output_range: Range<usize>,
        total_samples: usize,
        options: AudioMixOptions,
    ) -> Self {
        let mut missing = Vec::new();
        let resolved = audio_map
            .map(super::audio_map::ResolvedAudioMap::tracks)
            .unwrap_or_default();
        let rate = sample_rate as f64;
        let rescale = sample_rate as f64 / map_sample_rate.max(1) as f64;
        let to_output = |sample: usize| (sample as f64 * rescale).round() as usize;

        let voices: Vec<Range<f64>> = resolved
            .iter()
            .filter(|t| t.mix.voice)
            .map(|t| {
                to_output(t.range.start.as_usize()) as f64..to_output(t.range.end.as_usize()) as f64
            })
            .collect();

        let mut tracks = Vec::new();
        for track in resolved {
            let audio = match media.and_then(|m| m.resolve_audio(&track.file)) {
                Some(AudioData::Preloaded(data)) => data,
                Some(AudioData::Lazy) => continue,
                None => {
                    crate::diagnostics::report_missing_media(
                        crate::diagnostics::MediaKind::Audio,
                        &track.file,
                    );
                    missing.push(track.file.clone());
                    continue;
                }
            };

            let range =
                to_output(track.range.start.as_usize())..to_output(track.range.end.as_usize());
            if range.is_empty() || audio.sample_rate == 0 {
                continue;
            }

            let source_rate = f64::from(audio.sample_rate);
            let ratio = source_rate / rate;
            let source_start = f64::from(track.mix.offset) * source_rate;
            let natural_end =
                range.start as f64 + (audio.samples.len() as f64 - source_start) / ratio;
            let resampler = (audio.sample_rate as usize != sample_rate)
                .then(|| SincResampler::new(source_rate, rate));

            let pan = track.mix.pan.clamp(-1., 1.);
            let pan = if pan.abs() < 1e-6 {
                (1., 1.)
            } else if audio.is_stereo() {
                // Balance: attenuate the opposite side.
                let attenuation = (pan.abs() * std::f32::consts::FRAC_PI_2).cos();
                if pan > 0. {
                    (attenuation, 1.)
                } else {
                    (1., attenuation)
                }
            } else {
                // Equal power, compensated to unity at the center.
                let x = f32::midpoint(pan, 1.) * std::f32::consts::FRAC_PI_2;
                (
                    (std::f32::consts::SQRT_2 * x.cos()).min(1.),
                    (std::f32::consts::SQRT_2 * x.sin()).min(1.),
                )
            };

            let duck_under = match track.mix.duck {
                Some(ducking) => merge_ranges(
                    voices
                        .iter()
                        // A voice track does not duck itself.
                        .filter(|v| {
                            !(track.mix.voice && **v == (range.start as f64..range.end as f64))
                        })
                        .cloned()
                        .collect(),
                    f64::from(ducking.merge_gap) * rate,
                ),
                None => Vec::new(),
            };

            tracks.push(PreparedTrack {
                file: track.file.clone(),
                left: audio.samples.as_ref(),
                right: audio.right.as_deref(),
                source_start,
                ratio,
                resampler,
                gain: db_to_gain(track.mix.gain_db),
                pan,
                mix: track.mix,
                fade_in: f64::from(track.mix.fade_in) * rate,
                fade_out: f64::from(track.mix.fade_out) * rate,
                declick_in: options.declick && track.mix.offset > 0.,
                declick_out: options.declick && (range.end as f64) < natural_end - 1.,
                range,
                duck_under,
            });
        }

        Self {
            tracks,
            sample_rate,
            master_gain: db_to_gain(options.master_gain_db),
            limiter: options.limiter.map(|l| Limiter::new(l, sample_rate)),
            next_raw: None,
            output_range,
            total_samples,
            declick: options.declick,
            missing,
        }
    }

    /// Audio files of the map the media provider does not have.
    pub fn missing_files(&self) -> &[String] {
        &self.missing
    }

    pub fn sample_rate(&self) -> usize {
        self.sample_rate
    }

    /// Sums all tracks for `start..start + left.len()` without the master bus.
    fn mix_raw(&self, start: usize, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.);
        right.fill(0.);
        let end = start + left.len();
        let rate = self.sample_rate as f64;

        for track in &self.tracks {
            let from = track.range.start.max(start);
            let to = track.range.end.min(end);
            if from >= to {
                continue;
            }

            for n in from..to {
                let position = track.source_start + (n - track.range.start) as f64 * track.ratio;
                let gain = track.envelope(n, rate);
                if gain == 0. {
                    continue;
                }

                let i = n - start;
                let l = track.source_sample(track.left, position);
                if let Some(right_channel) = track.right {
                    let r = track.source_sample(right_channel, position);
                    left[i] += l * gain * track.pan.0;
                    right[i] += r * gain * track.pan.1;
                } else {
                    left[i] += l * gain * track.pan.0;
                    right[i] += l * gain * track.pan.1;
                }
            }
        }

        if self.master_gain != 1. {
            for s in left.iter_mut() {
                *s *= self.master_gain;
            }
            for s in right.iter_mut() {
                *s *= self.master_gain;
            }
        }
    }

    /// Fills `left`/`right` with the final mix of the timeline samples starting at `start`.
    pub fn render(&mut self, start: usize, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        let (left, right) = (&mut left[..len], &mut right[..len]);

        // The limiter is taken out while `mix_raw` borrows the mixer and put back after.
        if let Some(mut limiter) = self.limiter.take() {
            let latency = limiter.latency();
            let mut raw_l = vec![0.; len];
            let mut raw_r = vec![0.; len];

            // The limiter delays by `latency`, so it runs that far ahead of the output.
            if self.next_raw != Some(start + latency) {
                let mut prime_l = vec![0.; latency];
                let mut prime_r = vec![0.; latency];
                self.mix_raw(start, &mut prime_l, &mut prime_r);
                limiter.reset();
                for (l, r) in prime_l.into_iter().zip(prime_r) {
                    limiter.process(l, r);
                }
            }

            self.mix_raw(start + latency, &mut raw_l, &mut raw_r);
            for i in 0..len {
                let (l, r) = limiter.process(raw_l[i], raw_r[i]);
                left[i] = l;
                right[i] = r;
            }
            self.next_raw = Some(start + latency + len);
            self.limiter = Some(limiter);
        } else {
            self.mix_raw(start, left, right);
        }

        if self.declick {
            let edge = (DECLICK_SECONDS * self.sample_rate as f64) as usize;
            for i in 0..len {
                let n = start + i;
                let mut gain = 1.;
                if self.output_range.start > 0 && n < self.output_range.start + edge {
                    gain *= n.saturating_sub(self.output_range.start) as f32 / edge as f32;
                }
                if self.output_range.end < self.total_samples && n + edge >= self.output_range.end {
                    gain *= self.output_range.end.saturating_sub(n + 1) as f32 / edge as f32;
                }
                if gain < 1. {
                    left[i] *= gain;
                    right[i] *= gain;
                }
            }
        }
    }

    /// Mixes the whole output range into two channel buffers.
    pub fn render_all(&mut self) -> (Vec<f32>, Vec<f32>) {
        let len = self.output_range.len();
        let mut left = vec![0.; len];
        let mut right = vec![0.; len];
        let block = 4096;
        let mut offset = 0;
        while offset < len {
            let n = block.min(len - offset);
            let start = self.output_range.start + offset;
            self.render(
                start,
                &mut left[offset..offset + n],
                &mut right[offset..offset + n],
            );
            offset += n;
        }
        (left, right)
    }

    /// The tracks audible at a timeline sample with their position in the file and level.
    pub fn active_tracks_at(&self, sample: usize) -> Vec<ActiveTrack> {
        let rate = self.sample_rate as f64;
        self.tracks
            .iter()
            .filter(|t| t.range.contains(&sample))
            .map(|t| {
                let position = t.source_start + (sample - t.range.start) as f64 * t.ratio;
                let source_rate = t.ratio * rate;
                let gain = t.envelope(sample, rate);
                ActiveTrack {
                    file: t.file.clone(),
                    file_seconds: position / source_rate,
                    gain_db: if gain > 0. {
                        20. * gain.log10()
                    } else {
                        f32::NEG_INFINITY
                    },
                    voice: t.mix.voice,
                    ducked_db: t
                        .mix
                        .duck
                        .map_or(0., |d| duck_db(&t.duck_under, &d, sample as f64, rate)),
                }
            })
            .collect()
    }
}

/// A track playing at some moment, see `AudioMixer::active_tracks_at`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActiveTrack {
    pub file: String,
    /// Position inside the file.
    pub file_seconds: f64,
    /// Effective level (gain, fades and ducking) in dB.
    pub gain_db: f32,
    pub voice: bool,
    /// Current ducking attenuation (0 when not ducked).
    pub ducked_db: f32,
}

fn merge_ranges(mut ranges: Vec<Range<f64>>, gap: f64) -> Vec<Range<f64>> {
    ranges.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut merged: Vec<Range<f64>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end + gap => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioMap, AudioTimestamp::*, AudioTrack, TimeBase};
    use std::borrow::Cow;
    use std::collections::HashMap;

    #[derive(Debug)]
    struct Media(HashMap<String, AudioData<'static>>);

    impl<'a> MediaProvider<'a> for Media {
        fn resolve_audio(&'a self, name: &str) -> Option<&'a AudioData<'a>> {
            self.0.get(name)
        }
        fn resolve_image(&'a self, _: &str) -> Option<&'a crate::media::ImageData<'a>> {
            None
        }
        fn resolve_subtitles(&'a self, _: &str) -> Option<&'a crate::media::Subtitles<'a>> {
            None
        }
        fn resolve_video(&'a self, _: &str) -> Option<&'a crate::media::VideoMedia> {
            None
        }
        fn populate_font_source(&'a self, _: &mut dyn crate::FontSource) {}
        fn populate_image_source(
            &'a self,
            _: &mut HashMap<String, std::sync::Arc<crate::usvgr::PreloadedImageData>>,
        ) {
        }
    }

    fn audio(samples: Vec<f32>, sample_rate: u32) -> AudioData<'static> {
        AudioData::Preloaded(crate::media::PreloadedAudioData {
            samples: Cow::Owned(samples),
            sample_rate,
            right: None,
        })
    }

    const RATE: usize = 44100;

    fn resolve(map: AudioMap, media: &Media) -> ResolvedAudioMap<AudioTimelineSamples> {
        map.resolve_with_scenes(
            None,
            &TimeBase {
                fps: 30,
                sample_rate: RATE,
            },
            |file| Ok(f64::from(media.0[file].duration_in_seconds())),
        )
        .unwrap()
        .unwrap()
    }

    fn no_master() -> AudioMixOptions {
        AudioMixOptions {
            limiter: None,
            declick: false,
            ..Default::default()
        }
    }

    #[test]
    fn short_sounds_start_at_their_exact_sample() {
        // A click of 1000 samples at 0.51s: not frame aligned and in the middle of a 1024
        // sample encoder block. The old mixer dropped its beginning.
        let media = Media(HashMap::from([(
            "click".into(),
            audio(vec![0.5; 1000], RATE as u32),
        )]));
        let map = resolve(AudioMap::from([("click", Second(0.51)..Eof)]), &media);
        let start = (0.51 * RATE as f64).round() as usize;
        assert_eq!(map.tracks()[0].range.start.as_usize(), start);

        let mut mixer = AudioMixer::new(Some(&map), Some(&media), RATE, 0..RATE, RATE, no_master());
        let (left, right) = mixer.render_all();
        assert_eq!(left[start - 1], 0.);
        assert_eq!(left[start], 0.5);
        assert_eq!(right[start + 999], 0.5);
        assert_eq!(left[start + 1000], 0.);
        assert_eq!(left.iter().filter(|s| **s != 0.).count(), 1000);
    }

    #[test]
    fn overlapping_tracks_sum_linearly_and_the_limiter_catches_peaks() {
        let media = Media(HashMap::from([
            ("a".into(), audio(vec![0.6; RATE], RATE as u32)),
            ("b".into(), audio(vec![0.3; RATE], RATE as u32)),
        ]));
        let map = resolve(
            AudioMap::from([("a", Second(0.)..Eof), ("b", Second(0.)..Eof)]),
            &media,
        );

        let mut mixer = AudioMixer::new(Some(&map), Some(&media), RATE, 0..RATE, RATE, no_master());
        let (left, _) = mixer.render_all();
        assert!((left[100] - 0.9).abs() < 1e-6, "{}", left[100]);

        let loud = Media(HashMap::from([
            ("a".into(), audio(vec![0.9; RATE], RATE as u32)),
            ("b".into(), audio(vec![0.9; RATE], RATE as u32)),
        ]));
        let mut limited = AudioMixer::new(
            Some(&map),
            Some(&loud),
            RATE,
            0..RATE,
            RATE,
            AudioMixOptions::default(),
        );
        let (left, _) = limited.render_all();
        let ceiling = db_to_gain(-1.);
        assert!(left.iter().all(|s| s.abs() <= ceiling + 1e-6));
        assert!((left[RATE / 2] - ceiling).abs() < 1e-3);
    }

    #[test]
    fn gain_pan_fades_and_offset() {
        let ramp: Vec<f32> = (0..RATE).map(|i| i as f32 / RATE as f32).collect();
        let media = Media(HashMap::from([("ramp".into(), audio(ramp, RATE as u32))]));
        let map = resolve(
            AudioMap::from([AudioTrack::new("ramp", Second(0.)..Second(0.5))
                .offset(0.25)
                .gain_db(-6.0206)
                .pan(1.)]),
            &media,
        );
        let mut mixer = AudioMixer::new(Some(&map), Some(&media), RATE, 0..RATE, RATE, no_master());
        let (left, right) = mixer.render_all();

        // Offset: the first output sample is the file at 0.25s, at half the level.
        assert!((right[0] - 0.125).abs() < 1e-3, "{}", right[0]);
        // Hard right: nothing on the left.
        assert!(left.iter().all(|s| s.abs() < 1e-6));
        // Range end: silent after 0.5s.
        assert_eq!(right[RATE / 2 + 1], 0.);

        let media = Media(HashMap::from([(
            "tone".into(),
            audio(vec![1.; RATE], RATE as u32),
        )]));
        let map = resolve(
            AudioMap::from([AudioTrack::new("tone", Second(0.)..Eof)
                .fade_in(0.5)
                .fade_curve(crate::FadeCurve::Linear)]),
            &media,
        );
        let mut mixer = AudioMixer::new(Some(&map), Some(&media), RATE, 0..RATE, RATE, no_master());
        let (left, _) = mixer.render_all();
        assert!((left[RATE / 4] - 0.5).abs() < 1e-3);
        assert_eq!(left[RATE - 1], 1.);
    }

    #[test]
    fn ducking_follows_voice_ranges() {
        let media = Media(HashMap::from([
            ("music".into(), audio(vec![0.5; 4 * RATE], RATE as u32)),
            ("voice".into(), audio(vec![0.; RATE], RATE as u32)),
        ]));
        let map = resolve(
            AudioMap::from([
                AudioTrack::new("music", Second(0.)..Eof).duck_under_voice(),
                AudioTrack::new("voice", Second(1.)..Eof).voice(),
            ]),
            &media,
        );
        let mut mixer = AudioMixer::new(
            Some(&map),
            Some(&media),
            RATE,
            0..4 * RATE,
            4 * RATE,
            no_master(),
        );
        let (left, _) = mixer.render_all();

        assert!((left[RATE / 4] - 0.5).abs() < 1e-6, "before the attack");
        let ducked = 0.5 * db_to_gain(-12.);
        assert!(
            (left[RATE + RATE / 2] - ducked).abs() < 1e-4,
            "during the voice"
        );
        assert!(
            left[RATE - RATE / 10] < 0.5 && left[RATE - RATE / 10] > ducked,
            "attack"
        );
        // Voice ends at 2s, hold 0.3s, release 0.8s: back to full level at 3.1s.
        assert!(
            left[2 * RATE + RATE / 2] > ducked && left[2 * RATE + RATE / 2] < 0.5,
            "release"
        );
        assert!((left[3 * RATE + RATE / 5] - 0.5).abs() < 1e-6, "released");
    }

    #[test]
    fn resampling_keeps_a_tone_at_its_frequency_and_level() {
        let source_rate = 48000.;
        let tone: Vec<f32> = (0..48000)
            .map(|i| (2. * std::f64::consts::PI * 1000. * f64::from(i) / source_rate).sin() as f32)
            .collect();
        let media = Media(HashMap::from([("tone".into(), audio(tone, 48000))]));
        let map = resolve(AudioMap::from([("tone", Second(0.)..Eof)]), &media);
        let mut mixer = AudioMixer::new(Some(&map), Some(&media), RATE, 0..RATE, RATE, no_master());
        let (left, _) = mixer.render_all();

        for i in [1000usize, 20000, 40000] {
            let expected =
                (2. * std::f64::consts::PI * 1000. * i as f64 / RATE as f64).sin() as f32;
            assert!(
                (left[i] - expected).abs() < 2e-3,
                "{i}: {} vs {expected}",
                left[i]
            );
        }
    }

    #[test]
    fn maps_resolved_at_another_rate_are_rescaled() {
        let media = Media(HashMap::from([(
            "click".into(),
            audio(vec![0.5; 100], RATE as u32),
        )]));
        // Resolved at 44.1 kHz, mixed at 48 kHz: the click still starts at 0.5s.
        let map = resolve(AudioMap::from([("click", Second(0.5)..Eof)]), &media);
        let mut mixer = AudioMixer::new_rescaled(
            Some(&map),
            RATE,
            Some(&media),
            48000,
            0..48000,
            48000,
            no_master(),
        );
        let (left, _) = mixer.render_all();
        assert_eq!(left[23999], 0.);
        assert!(left[24000] > 0.4);
    }

    #[test]
    fn range_renders_start_in_the_middle_of_a_track() {
        let ramp: Vec<f32> = (0..RATE).map(|i| i as f32 / RATE as f32).collect();
        let media = Media(HashMap::from([("ramp".into(), audio(ramp, RATE as u32))]));
        let map = resolve(AudioMap::from([("ramp", Second(0.)..Eof)]), &media);
        let mut mixer = AudioMixer::new(
            Some(&map),
            Some(&media),
            RATE,
            RATE / 2..RATE,
            RATE,
            no_master(),
        );
        let (left, _) = mixer.render_all();
        assert_eq!(left.len(), RATE / 2);
        assert!((left[0] - 0.5).abs() < 1e-4);
    }
}
