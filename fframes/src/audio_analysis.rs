//! Measuring a mix: loudness (ITU-R BS.1770 / EBU R128), true peak, clipping and silence.
//! Use them to check a render's sound without listening to it.
//!
//! K-weighting follows libebur128's analog-prototype formulas (exact at 48 kHz, valid for any
//! rate), gating follows BS.1770 (-70 LUFS absolute, -10 LU relative, 400 ms blocks with 75%
//! overlap) and true peak uses the 4x polyphase interpolator of BS.1770 Annex 2.
use serde::Serialize;
use std::ops::Range;

/// Biquad in transposed direct form II, f64 state.
#[derive(Clone, Copy)]
pub(crate) struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    /// A filter from the coefficients of `H(z) = (b0 + b1/z + b2/z^2) / (a0 + a1/z + a2/z^2)`.
    pub(crate) fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b: b.map(|b| b / a[0]),
            a: [a[1] / a[0], a[2] / a[0]],
            z: [0.; 2],
        }
    }

    pub(crate) fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The two K-weighting stages for a sample rate (libebur128 `ebur128_init_filter`).
fn k_weighting(sample_rate: f64) -> [Biquad; 2] {
    let shelf = {
        let f0 = 1_681.974_450_955_533;
        let g = 3.999_843_853_973_347;
        let q = 0.707_175_236_955_419_6;
        let k = (std::f64::consts::PI * f0 / sample_rate).tan();
        let vh = 10f64.powf(g / 20.);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1. + k / q + k * k;
        Biquad {
            b: [
                (vh + vb * k / q + k * k) / a0,
                2. * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            a: [2. * (k * k - 1.) / a0, (1. - k / q + k * k) / a0],
            z: [0.; 2],
        }
    };
    let high_pass = {
        let f0 = 38.135_470_876_024_44;
        let q = 0.500_327_037_323_877_3;
        let k = (std::f64::consts::PI * f0 / sample_rate).tan();
        let a0 = 1. + k / q + k * k;
        Biquad {
            b: [1., -2., 1.],
            a: [2. * (k * k - 1.) / a0, (1. - k / q + k * k) / a0],
            z: [0.; 2],
        }
    };
    [shelf, high_pass]
}

/// BS.1770 Annex 2: 4x oversampling interpolator, 4 phases of 12 taps.
const TRUE_PEAK_PHASES: [[f64; 12]; 4] = [
    [
        0.001_708_984_375_0,
        0.010_986_328_125_0,
        -0.019_653_320_312_5,
        0.033_203_125_000_0,
        -0.059_448_242_187_5,
        0.137_329_101_562_5,
        0.972_167_968_750_0,
        -0.102_294_921_875_0,
        0.047_607_421_875_0,
        -0.026_611_328_125_0,
        0.014_892_578_125_0,
        -0.008_300_781_250_0,
    ],
    [
        -0.029_174_804_687_5,
        0.029_296_875_000_0,
        -0.051_757_812_500_0,
        0.089_111_328_125_0,
        -0.166_503_906_250_0,
        0.465_087_890_625_0,
        0.779_785_156_250_0,
        -0.200_317_382_812_5,
        0.101_562_500_000_0,
        -0.058_227_539_062_5,
        0.033_081_054_687_5,
        -0.018_920_898_437_5,
    ],
    [
        -0.018_920_898_437_5,
        0.033_081_054_687_5,
        -0.058_227_539_062_5,
        0.101_562_500_000_0,
        -0.200_317_382_812_5,
        0.779_785_156_250_0,
        0.465_087_890_625_0,
        -0.166_503_906_250_0,
        0.089_111_328_125_0,
        -0.051_757_812_500_0,
        0.029_296_875_000_0,
        -0.029_174_804_687_5,
    ],
    [
        -0.008_300_781_250_0,
        0.014_892_578_125_0,
        -0.026_611_328_125_0,
        0.047_607_421_875_0,
        -0.102_294_921_875_0,
        0.972_167_968_750_0,
        0.137_329_101_562_5,
        -0.059_448_242_187_5,
        0.033_203_125_000_0,
        -0.019_653_320_312_5,
        0.010_986_328_125_0,
        0.001_708_984_375_0,
    ],
];

fn true_peak(channel: &[f32]) -> f64 {
    let mut peak: f64 = channel.iter().fold(0., |m, s| m.max(f64::from(s.abs())));
    for n in 0..channel.len() {
        for phase in &TRUE_PEAK_PHASES {
            let mut acc = 0.;
            for (k, coefficient) in phase.iter().enumerate() {
                if n >= k {
                    acc += coefficient * f64::from(channel[n - k]);
                }
            }
            peak = peak.max(acc.abs());
        }
    }
    peak
}

fn to_db(linear: f64) -> f64 {
    if linear > 0. {
        20. * linear.log10()
    } else {
        f64::NEG_INFINITY
    }
}

fn energy_to_lufs(mean_square: f64) -> f64 {
    if mean_square > 0. {
        -0.691 + 10. * mean_square.log10()
    } else {
        f64::NEG_INFINITY
    }
}

/// Loudness values are `-inf` for digital silence; JSON gets `null` for them.
fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some((value * 100.).round() / 100.)
}

/// Per 100 ms energies of a K-weighted stereo signal, the base of every loudness value.
pub struct LoudnessAnalysis {
    sample_rate: usize,
    hop: usize,
    /// Sum over channels of the squared K-weighted samples in each 100 ms hop.
    hops: Vec<f64>,
}

impl LoudnessAnalysis {
    pub fn new(left: &[f32], right: &[f32], sample_rate: usize) -> Self {
        let hop = (sample_rate / 10).max(1);
        let mut hops = vec![0.; left.len() / hop];
        for channel in [left, right] {
            let [mut shelf, mut high_pass] = k_weighting(sample_rate as f64);
            for (i, sample) in channel.iter().enumerate().take(hops.len() * hop) {
                let y = high_pass.process(shelf.process(f64::from(*sample)));
                hops[i / hop] += y * y;
            }
        }
        Self {
            sample_rate,
            hop,
            hops,
        }
    }

    fn hop_range(&self, samples: Range<usize>) -> Range<usize> {
        (samples.start / self.hop).min(self.hops.len())
            ..(samples.end / self.hop).min(self.hops.len())
    }

    /// Mean square of windows of `hops_per_window` hops starting at every hop of `range`.
    fn windows(&self, range: Range<usize>, hops_per_window: usize) -> Vec<f64> {
        if range.len() < hops_per_window {
            return Vec::new();
        }
        (range.start..=range.end - hops_per_window)
            .map(|i| {
                self.hops[i..i + hops_per_window].iter().sum::<f64>()
                    / (hops_per_window * self.hop) as f64
            })
            .collect()
    }

    /// Integrated (gated) loudness of a part of the signal in LUFS.
    pub fn integrated(&self, samples: Range<usize>) -> f64 {
        let blocks = self.windows(self.hop_range(samples), 4);
        let absolute = 10f64.powf((-70. + 0.691) / 10.);
        let gated: Vec<f64> = blocks.into_iter().filter(|z| *z >= absolute).collect();
        if gated.is_empty() {
            return f64::NEG_INFINITY;
        }
        let relative = gated.iter().sum::<f64>() / gated.len() as f64 * 0.1;
        let kept: Vec<&f64> = gated.iter().filter(|z| **z >= relative).collect();
        energy_to_lufs(kept.iter().copied().sum::<f64>() / kept.len() as f64)
    }

    /// Momentary (400 ms) loudness every 100 ms.
    pub fn momentary(&self) -> Vec<f64> {
        self.windows(0..self.hops.len(), 4)
            .into_iter()
            .map(energy_to_lufs)
            .collect()
    }

    /// Short-term (3 s) loudness every 100 ms.
    pub fn short_term(&self) -> Vec<f64> {
        self.windows(0..self.hops.len(), 30)
            .into_iter()
            .map(energy_to_lufs)
            .collect()
    }

    /// Loudness range (EBU Tech 3342) in LU.
    pub fn loudness_range(&self) -> Option<f64> {
        let short_term: Vec<f64> = self
            .windows(0..self.hops.len(), 30)
            .into_iter()
            .filter(|z| *z >= 10f64.powf((-70. + 0.691) / 10.))
            .collect();
        if short_term.is_empty() {
            return None;
        }
        let relative = short_term.iter().sum::<f64>() / short_term.len() as f64 * 0.01;
        let mut kept: Vec<f64> = short_term
            .into_iter()
            .filter(|z| *z >= relative)
            .map(energy_to_lufs)
            .collect();
        kept.sort_by(f64::total_cmp);
        let n = kept.len();
        let low = kept[((n - 1) as f64 * 0.10).round() as usize];
        let high = kept[((n - 1) as f64 * 0.95).round() as usize];
        Some(high - low)
    }

    /// Ranges (seconds) of at least `min_seconds` where the momentary loudness is below
    /// `threshold_lufs`.
    pub fn quiet_ranges(&self, threshold_lufs: f64, min_seconds: f64) -> Vec<Range<f64>> {
        let hop_seconds = self.hop as f64 / self.sample_rate as f64;
        let mut ranges = Vec::new();
        let mut start = None;
        let momentary = self.momentary();
        for (i, loudness) in momentary.iter().chain([&f64::INFINITY]).enumerate() {
            match (start, *loudness < threshold_lufs) {
                (None, true) => start = Some(i),
                (Some(s), false) => {
                    // A momentary block covers 4 hops: the quiet part ends where the block
                    // that hears the sound begins plus its length.
                    let range = s as f64 * hop_seconds..(i + 3) as f64 * hop_seconds;
                    if range.end - range.start >= min_seconds {
                        ranges.push(range);
                    }
                    start = None;
                }
                _ => {}
            }
        }
        ranges
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionLoudness {
    pub name: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    /// Integrated loudness in LUFS, `null` for silence.
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
}

/// Everything `fframes audio analyze` reports.
#[derive(Debug, Clone, Serialize)]
pub struct AudioReport {
    pub duration_seconds: f64,
    pub sample_rate: usize,
    pub integrated_lufs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
    pub max_momentary_lufs: Option<f64>,
    pub max_short_term_lufs: Option<f64>,
    pub sample_peak_dbfs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    /// Samples beyond full scale (either channel), they distort once encoded.
    pub clipped_samples: usize,
    /// Parts quieter than -60 LUFS for at least a second.
    pub silent_ranges: Vec<[f64; 2]>,
    /// Loudness per scene or other named part.
    pub sections: Vec<SectionLoudness>,
}

/// Analyzes a stereo mix. `sections` are named sample ranges (usually the scenes).
pub fn analyze_audio(
    left: &[f32],
    right: &[f32],
    sample_rate: usize,
    sections: &[(String, Range<usize>)],
) -> AudioReport {
    let loudness = LoudnessAnalysis::new(left, right, sample_rate);
    let sample_peak = f64::from(left.iter().chain(right).fold(0f32, |m, s| m.max(s.abs())));
    let peak_of = |range: Range<usize>| {
        let range = range.start.min(left.len())..range.end.min(left.len());
        true_peak(&left[range.clone()]).max(true_peak(&right[range]))
    };
    let max = |values: Vec<f64>| values.into_iter().fold(f64::NEG_INFINITY, f64::max);

    AudioReport {
        duration_seconds: left.len() as f64 / sample_rate as f64,
        sample_rate,
        integrated_lufs: finite(loudness.integrated(0..left.len())),
        loudness_range_lu: loudness.loudness_range().and_then(finite),
        max_momentary_lufs: finite(max(loudness.momentary())),
        max_short_term_lufs: finite(max(loudness.short_term())),
        sample_peak_dbfs: finite(to_db(sample_peak)),
        true_peak_dbtp: finite(to_db(peak_of(0..left.len()))),
        clipped_samples: left.iter().chain(right).filter(|s| s.abs() > 1.).count(),
        silent_ranges: loudness
            .quiet_ranges(-60., 1.)
            .into_iter()
            .map(|r| {
                [
                    (r.start * 100.).round() / 100.,
                    (r.end * 100.).round() / 100.,
                ]
            })
            .collect(),
        sections: sections
            .iter()
            .map(|(name, range)| SectionLoudness {
                name: name.clone(),
                start_seconds: range.start as f64 / sample_rate as f64,
                end_seconds: range.end as f64 / sample_rate as f64,
                integrated_lufs: finite(loudness.integrated(range.clone())),
                true_peak_dbtp: finite(to_db(peak_of(range.clone()))),
            })
            .collect(),
    }
}

/// Encodes stereo samples as a WAV file: 16-bit PCM with TPDF dither (plays everywhere) or
/// 32-bit float (keeps levels above full scale).
pub fn encode_wav(left: &[f32], right: &[f32], sample_rate: usize, float: bool) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let channels: u16 = 2;
    let bytes_per_sample: u16 = if float { 4 } else { 2 };
    let data_len = frames * channels as usize * bytes_per_sample as usize;
    let mut out = Vec::with_capacity(data_len + 58);

    let fmt_len: u32 = if float { 18 } else { 16 };
    let fact_len: u32 = if float { 12 } else { 0 };
    let riff_len = 4 + (8 + fmt_len) + fact_len + 8 + data_len as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_len.to_le_bytes());
    out.extend_from_slice(&(if float { 3u16 } else { 1u16 }).to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&(sample_rate as u32).to_le_bytes());
    out.extend_from_slice(
        &(sample_rate as u32 * u32::from(channels) * u32::from(bytes_per_sample)).to_le_bytes(),
    );
    out.extend_from_slice(&(channels * bytes_per_sample).to_le_bytes());
    out.extend_from_slice(&(bytes_per_sample * 8).to_le_bytes());
    if float {
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(b"fact");
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&(frames as u32).to_le_bytes());
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());

    // Deterministic TPDF dither: two uniform values from a xorshift generator.
    let mut state: u32 = 0x9E37_79B9;
    let mut uniform = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32
    };

    for i in 0..frames {
        for sample in [left[i], right[i]] {
            if float {
                out.extend_from_slice(&sample.to_le_bytes());
            } else {
                let dither = uniform() - uniform();
                let value = (sample * 32767. + dither).round().clamp(-32768., 32767.) as i16;
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frequency: f64, amplitude: f32, seconds: f64, rate: usize) -> Vec<f32> {
        (0..(seconds * rate as f64) as usize)
            .map(|i| {
                amplitude
                    * (2. * std::f64::consts::PI * frequency * i as f64 / rate as f64).sin() as f32
            })
            .collect()
    }

    #[test]
    fn k_weighting_matches_the_spec_at_48k() {
        let [shelf, high_pass] = k_weighting(48000.);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
        assert!(close(shelf.b[0], 1.535_124_859_586_97));
        assert!(close(shelf.b[1], -2.691_696_189_406_38));
        assert!(close(shelf.b[2], 1.198_392_810_852_85));
        assert!(close(shelf.a[0], -1.690_659_293_182_41));
        assert!(close(shelf.a[1], 0.732_480_774_215_85));
        assert!(close(high_pass.a[0], -1.990_047_454_833_98));
        assert!(close(high_pass.a[1], 0.990_072_250_366_21));
    }

    #[test]
    fn full_scale_997hz_sine_in_one_channel_reads_minus_3_lufs() {
        // BS.1770: a 0 dBFS 997 Hz sine in one front channel measures -3.01 LKFS.
        let rate = 48000;
        let left = sine(997., 1., 5., rate);
        let right = vec![0.; left.len()];
        let report = analyze_audio(&left, &right, rate, &[]);
        let lufs = report.integrated_lufs.unwrap();
        assert!((lufs + 3.01).abs() < 0.05, "{lufs}");
        assert!(report.true_peak_dbtp.unwrap().abs() < 0.1);
        assert_eq!(report.clipped_samples, 0);
    }

    #[test]
    fn stereo_minus_23() {
        // EBU Tech 3341 test 1: stereo 1 kHz sine at -23 dBFS reads -23 LUFS.
        let rate = 48000;
        let tone = sine(1000., 10f32.powf(-23. / 20.), 20., rate);
        let report = analyze_audio(&tone, &tone, rate, &[]);
        let lufs = report.integrated_lufs.unwrap();
        assert!((lufs + 23.).abs() < 0.1, "{lufs}");
    }

    #[test]
    fn silence_and_sections() {
        let rate = 44100;
        let mut left = vec![0.; rate * 3];
        left.extend(sine(440., 0.5, 3., rate));
        let report = analyze_audio(
            &left,
            &left,
            rate,
            &[
                ("quiet".into(), 0..rate * 3),
                ("tone".into(), rate * 3..rate * 6),
            ],
        );
        assert_eq!(report.silent_ranges.len(), 1);
        assert!(report.silent_ranges[0][0] < 0.1 && report.silent_ranges[0][1] > 2.9);
        assert_eq!(report.sections[0].integrated_lufs, None);
        assert!(report.sections[1].integrated_lufs.is_some());
    }

    #[test]
    fn wav_headers() {
        let pcm = encode_wav(&[0.5, -0.5], &[0., 1.], 44100, false);
        assert_eq!(&pcm[0..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(pcm[4..8].try_into().unwrap()) as usize,
            pcm.len() - 8
        );
        assert_eq!(pcm.len(), 44 + 8);

        let float = encode_wav(&[0.5], &[0.25], 48000, true);
        assert_eq!(float.len(), 58 + 8);
        assert_eq!(u16::from_le_bytes(float[20..22].try_into().unwrap()), 3);
        assert_eq!(&float[58..62], &0.5f32.to_le_bytes());
    }
}
