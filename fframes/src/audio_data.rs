use crate::{audio_window_functions, fframes_context};
use std::{convert::TryInto, ops::Range};

#[derive(Debug, Clone)]
pub struct PreloadedAudioData {
    pub samples: Vec<i16>,
    pub sample_rate: i32,
}

impl PreloadedAudioData {
    pub fn duration_in_seconds(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    pub fn get_range(&self, range: std::ops::Range<usize>) -> Option<&[i16]> {
        self.samples.get(range)
    }

    pub fn get_frame_data(&self, length: usize, frame: usize, fps: i64) -> Option<&[i16]> {
        let start_index = frame * self.sample_rate as usize / fps as usize;

        self.samples.get(start_index..start_index + length)
    }
}

#[derive(Clone, Debug)]
pub enum AudioData {
    Preloaded(PreloadedAudioData),
    Lazy,
}

impl AudioData {
    pub fn duration_in_seconds(&self) -> f32 {
        match self {
            AudioData::Lazy => 0.,
            AudioData::Preloaded(data) => data.duration_in_seconds(),
        }
    }

    pub fn get_range(&self, range: Range<usize>) -> Option<&[i16]> {
        match self {
            AudioData::Preloaded(data) => data.get_range(range),
            AudioData::Lazy => None,
        }
    }

    pub fn get_frame_data(&self, length: usize, frame: usize, fps: i64) -> &[i16] {
        match self {
            AudioData::Preloaded(data) => data.get_frame_data(length, frame, fps).unwrap_or(&[]),
            AudioData::Lazy => &[],
        }
    }
}

#[derive(Debug)]
pub enum SampleSize {
    S2,
    S4,
    S8,
    S16,
    S32,
    S64,
    S128,
    S256,
    S512,
    S1024,
}

fn get_fft_size_number(variant: &SampleSize) -> usize {
    match variant {
        SampleSize::S2 => 2,
        SampleSize::S4 => 4,
        SampleSize::S8 => 8,
        SampleSize::S16 => 16,
        SampleSize::S32 => 32,
        SampleSize::S64 => 64,
        SampleSize::S128 => 128,
        SampleSize::S256 => 256,
        SampleSize::S512 => 512,
        SampleSize::S1024 => 1024,
    }
}

#[derive(Debug)]
pub struct VisualizeFrameInput<'a> {
    pub audio: &'a AudioData,
    pub sample_size: SampleSize,
    pub smooth_level: usize,
    pub ctx: &'a fframes_context::FFramesContext<'a>,
    pub window: Option<audio_window_functions::WindowFunction>,
}

#[allow(clippy::unnecessary_lazy_evaluations)]
fn apply_fft_to_frame(
    sample_size: &SampleSize,
    window: &Option<audio_window_functions::WindowFunction>,
    audio_data: &AudioData,
    frame: usize,
    fps: i64,
) -> Vec<microfft::Complex32> {
    let apply_window = |size| -> Vec<f32> {
        let samples_per_frame = audio_data.get_frame_data(size, frame, fps);
        if let Some(window_function) = window {
            audio_window_functions::apply_window_function(*window_function, samples_per_frame)
        } else {
            samples_per_frame
                .iter()
                .map(|sample| *sample as f32)
                .collect()
        }
    };

    match sample_size {
        SampleSize::S2 => {
            let mut buffer: [_; 2] = apply_window(2).try_into().unwrap_or_else(|_| [0.0; 2]);
            microfft::real::rfft_2(&mut buffer).to_vec()
        }
        SampleSize::S4 => {
            let mut buffer: [_; 4] = apply_window(4).try_into().unwrap_or_else(|_| [0.0; 4]);
            microfft::real::rfft_4(&mut buffer).to_vec()
        }
        SampleSize::S8 => {
            let mut buffer: [_; 8] = apply_window(8).try_into().unwrap_or_else(|_| [0.0; 8]);
            microfft::real::rfft_8(&mut buffer).to_vec()
        }
        SampleSize::S16 => {
            let mut buffer: [_; 16] = apply_window(16).try_into().unwrap_or_else(|_| [0.0; 16]);
            microfft::real::rfft_16(&mut buffer).to_vec()
        }
        SampleSize::S32 => {
            let mut buffer: [_; 32] = apply_window(32).try_into().unwrap_or_else(|_| [0.0; 32]);

            microfft::real::rfft_32(&mut buffer).to_vec()
        }
        SampleSize::S64 => {
            let mut buffer: [_; 64] = apply_window(64).try_into().unwrap_or_else(|_| [0.0; 64]);
            microfft::real::rfft_64(&mut buffer).to_vec()
        }
        SampleSize::S128 => {
            let mut buffer: [_; 128] = apply_window(128).try_into().unwrap_or_else(|_| [0.0; 128]);

            microfft::real::rfft_128(&mut buffer).to_vec()
        }
        SampleSize::S256 => {
            let mut buffer: [_; 256] = apply_window(256).try_into().unwrap_or_else(|_| [0.0; 256]);
            microfft::real::rfft_256(&mut buffer).to_vec()
        }
        SampleSize::S512 => {
            let mut buffer: [_; 512] = apply_window(512).try_into().unwrap_or_else(|_| [0.0; 512]);
            microfft::real::rfft_512(&mut buffer).to_vec()
        }
        SampleSize::S1024 => {
            let mut buffer: [_; 1024] = apply_window(1024)
                .try_into()
                .unwrap_or_else(|_| [0.0; 1024]);
            microfft::real::rfft_1024(&mut buffer).to_vec()
        }
    }
}

pub fn get_visualization(
    frame: usize,
    VisualizeFrameInput {
        sample_size,
        ctx,
        audio,
       window,
        ..
    }: &VisualizeFrameInput,
) -> Vec<f32> {
    // TODO cache the results per frame to avoid same frame calculation when smoothing
    let fft_size = get_fft_size_number(sample_size);

    let res = apply_fft_to_frame(sample_size, window, audio, frame, ctx.fps as i64)
        .iter()
        .map(|x| x.norm() / fft_size as f32)
        .collect::<Vec<f32>>();

    res
}

/// Prettifies audio spectrum by moving low frequences (first elements) in the middle and all the other
/// elements to be proportionally positioned to the edges from the middle
///
/// Stranger if you are reading this comment you might be interested in implementation and how
/// to make it more efficient and faster. Here is a great place to help fframes by changing implementation
/// of this function to be in-place and do not allocate.
pub fn prettify_spectrum(spectrum: &[f32]) -> Vec<f32> {
    let mut pretty_spectrum = vec![0.0; spectrum.len()];
    let mid = spectrum.len() / 2 - 1;

    for (i, _) in spectrum.iter().enumerate() {
        pretty_spectrum[i] = match i {
            i if i < mid => spectrum[(mid - i) * 2],
            i if i == mid => spectrum[0],
            _ => spectrum[(i - mid) * 2 - 1],
        };
    }

    pretty_spectrum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prettify_spectrum() {
        assert_eq!(
            prettify_spectrum([1., 2., 3., 4., 5., 6., 7., 8.].as_slice()),
            vec![7., 5., 3., 1., 2., 4., 6., 8.]
        );
    }
}
