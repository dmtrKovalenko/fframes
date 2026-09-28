use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct PreloadedAudioData<'a> {
    /// The only channel of a mono file or the left channel of a stereo file.
    pub samples: Cow<'a, [f32]>,
    pub sample_rate: u32,
    /// The right channel of a stereo file, same length as `samples`.
    pub right: Option<Cow<'a, [f32]>>,
}

impl PreloadedAudioData<'_> {
    pub fn is_stereo(&self) -> bool {
        self.right.is_some()
    }

    /// `(left, right)`; for mono files both are the same channel.
    pub fn channels(&self) -> (&[f32], &[f32]) {
        let left = self.samples.as_ref();
        (left, self.right.as_deref().unwrap_or(left))
    }

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

    /// Like `get_frame_data` but averages both channels of stereo files.
    pub fn get_frame_data_mono(
        &self,
        length: usize,
        frame: usize,
        fps: i64,
    ) -> Option<Cow<'_, [f32]>> {
        let start_index = frame * self.sample_rate as usize / fps as usize;
        let range = start_index..start_index + length;
        let left = self.samples.get(range.clone())?;
        match self.right.as_deref() {
            Some(right) => Some(Cow::Owned(
                left.iter()
                    .zip(right.get(range)?)
                    .map(|(l, r)| (l + r) * 0.5)
                    .collect(),
            )),
            None => Some(Cow::Borrowed(left)),
        }
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
            right: None,
        })
    }

    /// Decodes a file keeping stereo: mono files stay mono, stereo files keep both channels
    /// and files with more channels are downmixed to stereo.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn decode_raw_file_stereo(
        sample_rate: Option<u32>,
        filename: &std::path::PathBuf,
    ) -> crate::error::Result<Self> {
        use crate::audio_decoder::{AudioDecoder, ChannelMode};

        let mut decoder =
            AudioDecoder::new_with_channels(filename, sample_rate, ChannelMode::KeepStereo)?;
        let (sample_rate, mut channels) = decoder.decode_all_channels()?;
        let right = (channels.len() > 1).then(|| Cow::Owned(channels.pop().unwrap_or_default()));
        let samples = channels.pop().unwrap_or_default();

        Ok(PreloadedAudioData {
            samples: Cow::Owned(samples),
            sample_rate,
            right,
        })
    }
}
