use core::f32::consts::PI;
use libm::cosf;

/// Applies a Hann window (<https://en.wikipedia.org/wiki/Window_function#Hann_and_Hamming_windows>)
/// to an array of samples.
///
/// ## Return value
/// New vector with Hann window applied to the values.
pub fn hann_window(samples: &[f32]) -> Vec<f32> {
    let mut windowed_samples = Vec::with_capacity(samples.len());
    for i in 0..samples.len() {
        let multiplier = 0.5 * (1.0 - cosf((2.0 * PI * i as f32) / (samples.len() as f32)));

        windowed_samples.push(multiplier * samples[i])
    }

    windowed_samples
}

/// Applies a Hamming window (<https://en.wikipedia.org/wiki/Window_function#Hann_and_Hamming_windows>)
/// to an array of samples.
///
/// ## Return value
/// New vector with Hann window applied to the values.
pub fn hamming_window(samples: &[f32]) -> Vec<f32> {
    let mut windowed_samples = Vec::with_capacity(samples.len());
    let samples_len_f32 = samples.len() as f32;
    for (i, sample) in samples.iter().enumerate() {
        let multiplier = 0.54 - (0.46 * (2.0 * PI * i as f32 / cosf(samples_len_f32 - 1.0)));
        windowed_samples.push(multiplier * *sample)
    }
    windowed_samples
}

#[derive(Debug, Clone, Copy)]
pub enum WindowFunction {
    Hann,
    Hamming,
}

pub fn apply_window_function(window: WindowFunction, samples: &[f32]) -> Vec<f32> {
    match window {
        WindowFunction::Hann => hann_window(samples),
        WindowFunction::Hamming => hamming_window(samples),
    }
}
