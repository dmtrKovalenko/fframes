use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct PreloadedAudioData<'a> {
    pub samples: Cow<'a, [f32]>,
    pub sample_rate: u32,
}

impl PreloadedAudioData<'_> {
    pub fn duration_in_seconds(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    pub fn duration_in_frames(&self, fps: usize) -> usize {
        self.samples.len() * fps / self.sample_rate as usize
    }

    pub fn get_range(&self, range: std::ops::Range<usize>) -> Option<&[f32]> {
        self.samples.get(range)
    }

    pub fn get_frame_data(&self, length: usize, frame: usize, fps: i64) -> Option<&[f32]> {
        let start_index = frame * self.sample_rate as usize / fps as usize;

        self.samples.get(start_index..start_index + length)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn decode_raw_file(
        sample_rate: Option<u32>,
        filename: &std::path::PathBuf,
    ) -> crate::error::Result<Self> {
        use crate::audio_decoder::AudioDecoder;

        let mut decoder = AudioDecoder::new(filename, sample_rate)?;
        let (sample_rate, samples) = decoder.decode_all_samples()?;

        Ok(PreloadedAudioData {
            samples: Cow::Owned(samples),
            sample_rate,
        })
    }
}
