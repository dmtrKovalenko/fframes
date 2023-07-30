use fframes::PreloadedAudioData;
use minimp3::{Decoder as Mp3Decoder, Error as Mp3Error, Frame as Mp3Frame};
use std::{fs::File, path::Path};

use crate::renderer_error::FFramesRendererResult;

pub fn decode_mp3<P: AsRef<Path>>(audio_path: P) -> FFramesRendererResult<PreloadedAudioData> {
    let mut decoder = Mp3Decoder::new(File::open(audio_path)?);

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
            Err(e) => panic!("{:?}", e),
        }
    }

    Ok(PreloadedAudioData {
        samples: mono_samples,
        sample_rate,
    })
}
