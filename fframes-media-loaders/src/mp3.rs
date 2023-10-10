use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct PreloadedAudioData<'a> {
    pub samples: Cow<'a, [i16]>,
    pub sample_rate: i32,
}

impl PreloadedAudioData<'_> {
    pub fn duration_in_seconds(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    pub fn duration_in_frames(&self, fps: usize) -> usize {
        self.samples.len() * fps / self.sample_rate as usize
    }

    pub fn get_range(&self, range: std::ops::Range<usize>) -> Option<&[i16]> {
        self.samples.get(range)
    }

    pub fn get_frame_data(&self, length: usize, frame: usize, fps: i64) -> Option<&[i16]> {
        let start_index = frame * self.sample_rate as usize / fps as usize;

        self.samples.get(start_index..start_index + length)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decode_mp3(buf: impl std::io::Read) -> crate::error::Result<PreloadedAudioData<'static>> {
    use minimp3::{Decoder as Mp3Decoder, Error as Mp3Error, Frame as Mp3Frame};

    let mut decoder = Mp3Decoder::new(buf);

    let mut sample_rate = 0;
    let mut mono_samples = Vec::new();
    loop {
        match decoder.next_frame() {
            Ok(Mp3Frame {
                data: samples_of_frame,
                sample_rate: sample_rate_of_frame,
                channels,
                ..
            }) => {
                // Sampling rate can not change over mp3 file
                sample_rate = sample_rate_of_frame;

                match channels {
                    1 => samples_of_frame
                        .into_iter()
                        .for_each(|sample| mono_samples.push(sample)),
                    channels => {
                        for (i, sample) in samples_of_frame.iter().enumerate().step_by(channels) {
                            let sample = *sample;
                            let next_sample = samples_of_frame[i + 1];

                            // get the average (sample + next_sample) / 2 without overflow
                            mono_samples
                                .push((sample & next_sample) + ((sample ^ next_sample) >> 1));
                        }
                    }
                }
            }
            Err(Mp3Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(PreloadedAudioData {
        samples: Cow::Owned(mono_samples),
        sample_rate,
    })
}
