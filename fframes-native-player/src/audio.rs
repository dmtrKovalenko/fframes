use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use fframes::{
    AudioMixOptions, AudioMixer, AudioTimelineSamples, AudioTimelineUnit, FFramesContext,
    ResolvedAudioMap,
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Samples mixed per step, same order of magnitude as an encoder audio frame.
const CHUNK: usize = 1024;

pub(crate) struct AudioOutput {
    // Not `Send` on every platform, lives on the main thread.
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    /// The write end of the sample ring, used by the feeder thread only.
    producer: Mutex<Producer<Sample>>,
    pub(crate) sample_rate: usize,
}

/// A mixed stereo sample, tagged with the playback it was mixed for.
#[derive(Clone, Copy)]
struct Sample {
    left: f32,
    right: f32,
    epoch: u64,
}

struct Shared {
    control: Mutex<Control>,
    feeder_wakeup: Condvar,
    /// Read by the audio callback without a lock. Both change under `control`.
    playing: AtomicBool,
    /// Bumped on every seek. The callback discards samples of earlier epochs.
    epoch: AtomicU64,
    /// How many mixed samples to keep queued (~100ms).
    target_len: usize,
}

struct Control {
    looping: bool,
    /// Next position to mix, in samples since the start of the (possibly looped) playback.
    feed_position: u64,
    shutdown: bool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Control> {
        self.control
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
        let target_len = sample_rate / 10;

        let shared = Arc::new(Shared {
            control: Mutex::new(Control {
                looping: false,
                feed_position: 0,
                shutdown: false,
            }),
            feeder_wakeup: Condvar::new(),
            playing: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            target_len,
        });
        // Room for the samples of the current playback plus the ones of the playback
        // before a seek, which the callback discards.
        let (producer, consumer) = RingBuffer::new(2 * target_len + CHUNK);

        let config = config.config();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                build_stream::<f32>(&device, config, channels, &shared, consumer)
            }
            cpal::SampleFormat::I16 => {
                build_stream::<i16>(&device, config, channels, &shared, consumer)
            }
            cpal::SampleFormat::U16 => {
                build_stream::<u16>(&device, config, channels, &shared, consumer)
            }
            cpal::SampleFormat::I32 => {
                build_stream::<i32>(&device, config, channels, &shared, consumer)
            }
            format => return Err(format!("unsupported sample format {format}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            shared,
            producer: Mutex::new(producer),
            sample_rate,
        })
    }

    /// Restarts the audio at `position` (in frames of the player position space).
    pub(crate) fn reset(&self, position: u64, fps: usize, playing: bool, looping: bool) {
        let mut control = self.shared.lock();
        control.feed_position = position * self.sample_rate as u64 / fps as u64;
        control.looping = looping;
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        self.shared.playing.store(playing, Ordering::Release);
        drop(control);
        self.shared.feeder_wakeup.notify_all();
    }

    pub(crate) fn shutdown(&self) {
        self.shared.lock().shutdown = true;
        self.shared.feeder_wakeup.notify_all();
    }

    /// Mixes audio into the ring until `shutdown`. Runs on a scoped thread.
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
        let mut producer = self
            .producer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let capacity = producer.buffer().capacity();

        loop {
            let (position, looping, epoch) = {
                let mut control = self.shared.lock();
                loop {
                    if control.shutdown {
                        return;
                    }
                    let has_more = control.looping || control.feed_position < total_samples;
                    let queued = capacity - producer.slots();
                    if self.shared.playing.load(Ordering::Acquire)
                        && has_more
                        && queued < self.shared.target_len
                    {
                        break;
                    }
                    control = self
                        .shared
                        .feeder_wakeup
                        .wait(control)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                (
                    control.feed_position,
                    control.looping,
                    self.shared.epoch.load(Ordering::Acquire),
                )
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

            let mut control = self.shared.lock();
            // The wait above leaves at least a chunk of room in the ring.
            if self.shared.epoch.load(Ordering::Acquire) == epoch
                && let Ok(chunk) = producer.write_chunk_uninit(len)
            {
                let samples = left[..len]
                    .iter()
                    .zip(&right[..len])
                    .map(|(&left, &right)| Sample { left, right, epoch });
                control.feed_position += chunk.fill_from_iter(samples) as u64;
            }
        }
    }
}

fn build_stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    shared: &Arc<Shared>,
    mut consumer: Consumer<Sample>,
) -> Result<cpal::Stream, String> {
    let shared = Arc::clone(shared);

    device
        .build_output_stream(
            config,
            // The callback never blocks: it reads the ring and two atomics.
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let epoch = shared.epoch.load(Ordering::Acquire);
                let playing = shared.playing.load(Ordering::Acquire);

                for frame in data.chunks_mut(channels) {
                    // Samples mixed before the last seek are discarded, also while paused.
                    while consumer.peek().is_ok_and(|sample| sample.epoch != epoch) {
                        let _ = consumer.pop();
                    }
                    let (left, right) = if playing {
                        consumer
                            .pop()
                            .map_or((0.0, 0.0), |sample| (sample.left, sample.right))
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

                if playing && consumer.slots() < shared.target_len {
                    shared.feeder_wakeup.notify_one();
                }
            },
            |err| eprintln!("fframes player: audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
