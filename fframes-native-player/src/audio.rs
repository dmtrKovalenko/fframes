use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use fframes::{
    AudioMixOptions, AudioMixer, AudioTimelineSamples, AudioTimelineUnit, FFramesContext,
    ResolvedAudioMap,
};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Samples mixed per step, same order of magnitude as an encoder audio frame.
const CHUNK: usize = 1024;

pub(crate) struct AudioOutput {
    // Not `Send` on every platform, lives on the main thread.
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    pub(crate) sample_rate: usize,
}

struct Shared {
    queue: Mutex<Queue>,
    feeder_wakeup: Condvar,
    /// How many mixed samples to keep queued (~100ms).
    target_len: usize,
}

struct Queue {
    /// Stereo frames `(left, right)`.
    samples: VecDeque<(f32, f32)>,
    playing: bool,
    looping: bool,
    /// Next position to mix, in samples since the start of the (possibly looped) playback.
    feed_position: u64,
    epoch: u64,
    shutdown: bool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl AudioOutput {
    pub(crate) fn new() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let sample_rate = config.sample_rate() as usize;
        let channels = config.channels() as usize;
        let sample_format = config.sample_format();

        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                samples: VecDeque::new(),
                playing: false,
                looping: false,
                feed_position: 0,
                epoch: 0,
                shutdown: false,
            }),
            feeder_wakeup: Condvar::new(),
            target_len: sample_rate / 10,
        });

        let config = config.config();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, channels, &shared),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, channels, &shared),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, channels, &shared),
            cpal::SampleFormat::I32 => build_stream::<i32>(&device, config, channels, &shared),
            format => return Err(format!("unsupported sample format {format}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            shared,
            sample_rate,
        })
    }

    /// Restarts the audio at `position` (in frames of the player position space).
    pub(crate) fn reset(&self, position: u64, fps: usize, playing: bool, looping: bool) {
        let mut queue = self.shared.lock();
        queue.epoch += 1;
        queue.samples.clear();
        queue.feed_position = position * self.sample_rate as u64 / fps as u64;
        queue.playing = playing;
        queue.looping = looping;
        drop(queue);
        self.shared.feeder_wakeup.notify_all();
    }

    pub(crate) fn shutdown(&self) {
        self.shared.lock().shutdown = true;
        self.shared.feeder_wakeup.notify_all();
    }

    /// Mixes audio into the queue until `shutdown`. Runs on a scoped thread.
    pub(crate) fn run_feeder(
        &self,
        ctx: &FFramesContext<'_, '_>,
        audio_map: &ResolvedAudioMap<AudioTimelineSamples>,
        mix_options: AudioMixOptions,
    ) {
        let total_samples =
            AudioTimelineSamples::from_frames(ctx.duration_in_frames, &ctx.time_base)
                .as_usize()
                .max(1) as u64;
        // The same mixer as the encoder: what plays here is what gets rendered.
        let mut mixer = AudioMixer::new(
            Some(audio_map),
            ctx.media_source,
            self.sample_rate,
            0..total_samples as usize,
            total_samples as usize,
            mix_options,
        );
        for file in mixer.missing_files() {
            eprintln!("fframes player: audio \"{file}\" is not in the media provider");
        }
        let mut left = vec![0.; CHUNK];
        let mut right = vec![0.; CHUNK];

        loop {
            let (position, looping, epoch) = {
                let mut queue = self.shared.lock();
                loop {
                    if queue.shutdown {
                        return;
                    }
                    let has_more = queue.looping || queue.feed_position < total_samples;
                    if queue.playing && has_more && queue.samples.len() < self.shared.target_len {
                        break;
                    }
                    queue = self
                        .shared
                        .feeder_wakeup
                        .wait(queue)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                (queue.feed_position, queue.looping, queue.epoch)
            };

            let timeline_sample = if looping {
                position % total_samples
            } else {
                position
            };
            // Do not mix across the end of the video in one chunk.
            let len = CHUNK.min((total_samples - timeline_sample) as usize);

            // The mixer keeps limiter state between consecutive chunks and resets it on seeks.
            mixer.render(
                timeline_sample as usize,
                &mut left[..len],
                &mut right[..len],
            );

            let mut queue = self.shared.lock();
            if queue.epoch == epoch {
                queue.samples.extend(
                    left[..len]
                        .iter()
                        .copied()
                        .zip(right[..len].iter().copied()),
                );
                queue.feed_position += len as u64;
            }
        }
    }
}

fn build_stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, String> {
    let shared = Arc::clone(shared);

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut queue = shared.lock();
                let playing = queue.playing;

                for frame in data.chunks_mut(channels) {
                    let (left, right) = if playing {
                        queue.samples.pop_front().unwrap_or((0.0, 0.0))
                    } else {
                        (0.0, 0.0)
                    };
                    match frame {
                        [mono] => *mono = T::from_sample(f32::midpoint(left, right)),
                        [l, r, rest @ ..] => {
                            *l = T::from_sample(left);
                            *r = T::from_sample(right);
                            // Surround outputs: stereo on the front pair only.
                            rest.fill(T::from_sample(0.0));
                        }
                        [] => {}
                    }
                }

                let needs_more = playing && queue.samples.len() < shared.target_len;
                drop(queue);
                if needs_more {
                    shared.feeder_wakeup.notify_one();
                }
            },
            |err| eprintln!("fframes player: audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
