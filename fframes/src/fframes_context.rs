use crate::media::{ImageData, Subtitles};
use crate::{
    AudioData, AudioTimelineSamples, AudioTimelineUnit, FontSource, Frame, MediaProvider,
    ResolvedAudioMap, ResolvedScenesTimeline, Svgr,
};
use fframes_media::VideoMedia;
use std::iter::FromIterator;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug)]
pub enum FFramesMode {
    Editor,
    EditorTimelinePreview,
    Renderer,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeBase {
    pub fps: usize,
    pub sample_rate: usize,
}

#[derive(Debug, Clone)]
pub struct VideoSize {
    pub width: usize,
    pub height: usize,
}

impl VideoSize {
    pub fn new_scaled(width: usize, height: usize, scale: f64) -> Self {
        if scale == 1. {
            return Self { width, height };
        }

        Self {
            width: (width as f64 * scale) as usize,
            height: (height as f64 * scale) as usize,
        }
    }
}

#[derive(Debug)]
pub struct FFramesContext<'a, 'media: 'a> {
    /// Actual time base base of the video contains the FPS for video and sample rate for audio.
    pub time_base: TimeBase,
    /// The video size might be overridden by the render options.
    /// Use this field to get the most up-to-date video size and scale the SVG using viewbox.
    pub current_video_size: VideoSize,
    /// Total duration of the video in frames.
    pub duration_in_frames: usize,
    /// The execution mode: Editor, EditorTimelinePreview, or Renderer.
    pub mode: FFramesMode,
    /// Resolved scenes timeline if provided by the Video implementation
    pub scenes: Option<&'a ResolvedScenesTimeline<'a>>,
    /// Media source can be used to resolve audio, video, images, and any other supported media
    pub media_source: Option<&'media dyn MediaProvider<'media>>,
    pub font_source: Option<&'a (dyn FontSource<'a> + 'a)>,
    pub abort_signal: Option<&'media AbortSignal>,
}

impl<'a, 'media: 'a> FFramesContext<'a, 'media> {
    pub fn get_audio(&self, filename: impl AsRef<str>) -> Option<&'media AudioData<'media>> {
        self.media_source?.resolve_audio(filename.as_ref())
    }

    pub fn get_subtitles(&self, filename: impl AsRef<str>) -> Option<&'media Subtitles<'media>> {
        self.media_source?.resolve_subtitles(filename.as_ref())
    }

    pub fn get_image(&self, filename: impl AsRef<str>) -> Option<&'media ImageData<'media>> {
        self.media_source?.resolve_image(filename.as_ref())
    }

    pub fn get_video(&self, filename: impl AsRef<str>) -> Option<&'media VideoMedia> {
        self.media_source?.resolve_video(filename.as_ref())
    }

    pub fn render_scenes(&self, global_frame: &Frame) -> Svgr<'a> {
        if let Some(scenes) = self.scenes.as_ref() {
            Svgr::from_iter(
                scenes
                    .iter()
                    .filter(|&(range, _, _scene)| range.contains(&global_frame.index))
                    .map(|(range, _, scene)| {
                        scene.render_frame(
                            Frame::clone_with_scene_offset(global_frame, range.start),
                            self,
                        )
                    }),
            )
        } else {
            Svgr::default()
        }
    }

    /// Finds the scene layout and duration information based on the layout of defined in `define_scenes` of the `Video`.
    pub fn get_scene_info<T: crate::Scene>(&self, scene: &T) -> Option<&crate::SceneInfo> {
        let scenes = self.scenes.as_ref()?;
        scenes.iter().find_map(|(_, info, boxed_scene)| {
            #[allow(clippy::ptr_eq)]
            let pointers_equal =
                *boxed_scene as *const dyn crate::Scene as *const T == scene as *const T;

            pointers_equal.then_some(info)
        })
    }

    /// This is internal method that is used by the renderer which mixes audio data and returns the final as fltp in a vector.
    #[allow(clippy::option_map_unit_fn)]
    pub fn get_mixed_audio_data_in_fltp(
        &self,
        audio_map: &ResolvedAudioMap<AudioTimelineSamples>,
        start_sample: AudioTimelineSamples,
        frame_size: usize,
    ) -> Vec<f32> {
        let mut audio_data = vec![0.0; frame_size];
        let encoder_rate = self.time_base.sample_rate;

        let media_source = if let Some(media_source) = self.media_source {
            media_source
        } else {
            return vec![];
        };

        audio_map.0.iter().for_each(|(f, sample_range)| {
            if sample_range.contains(&start_sample) {
                let start_of_this_frame_in_file =
                    start_sample.as_usize() - sample_range.start.as_usize();

                let audio = media_source.resolve_audio(f);

                if let Some(audio) = audio {
                    let source_rate = audio.sample_rate() as usize;

                    if source_rate == 0 {
                        return;
                    }

                    if source_rate == encoder_rate {
                        // Fast path: same sample rate, direct indexing
                        let range = audio.get_range(
                            start_of_this_frame_in_file..start_of_this_frame_in_file + frame_size,
                        );

                        if let Some(range) = range {
                            Self::mix_audio_samples(&mut audio_data, range);
                        }
                    } else {
                        // Resampling path: source and encoder have different sample rates.
                        // Use linear interpolation to produce frame_size output samples from the
                        // corresponding span of source samples.
                        let ratio = source_rate as f64 / encoder_rate as f64;
                        let source_start = (start_of_this_frame_in_file as f64 * ratio) as usize;
                        let source_needed = ((frame_size as f64 * ratio).ceil() as usize) + 1;

                        let range = audio.get_range(source_start..source_start + source_needed);

                        if let Some(range) = range {
                            for (i, output_sample) in
                                audio_data.iter_mut().enumerate().take(frame_size)
                            {
                                let exact_pos = i as f64 * ratio;
                                let idx = exact_pos as usize;
                                let frac = (exact_pos - idx as f64) as f32;

                                let sample = if idx + 1 < range.len() {
                                    range[idx] * (1.0 - frac) + range[idx + 1] * frac
                                } else if idx < range.len() {
                                    range[idx]
                                } else {
                                    0.0
                                };

                                if *output_sample == 0. {
                                    *output_sample = sample;
                                } else {
                                    *output_sample =
                                        *output_sample + sample - (*output_sample * sample);
                                }
                            }
                        }
                    }
                }
            }
        });

        audio_data
    }

    #[inline]
    fn mix_audio_samples(audio_data: &mut [f32], range: &[f32]) {
        range.iter().enumerate().for_each(|(i, sample)| {
            let filled_sample = audio_data[i];

            if filled_sample == 0. {
                audio_data[i] = *sample
            } else {
                audio_data[i] = filled_sample + *sample - (filled_sample * *sample)
            }
        });
    }
}

/// A signal that can be used to abort the rendering process from the other thread to stop the
/// rendering process or to abort the rendering process inside the rendering without panicking.
///
/// ```no_run
/// let abort_signal = fframes::AbortSignal::new();
/// let signal_clone = Arc::new(abort_signal.clone());
///
/// std::thread::spawn(move || {
///     std::thread::sleep(std::time::Duration::from_secs(3));
///     println!("Aborting rendering...");
///     signal_clone.abort();
/// });
///
/// fframes::render(
///    "out.mp4",
///    YourVideo::new(),
///    fframes::RenderOptions {
///        abort_signal: Some(&abort_signal),
///    }
/// )
/// ```
#[derive(Debug)]
pub struct AbortSignal {
    abort: AtomicBool,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            abort: AtomicBool::new(false),
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.abort.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// If the abort signal was populated at the `fframes::render` level it is possible
    /// to abort the rendering process without panicking the thread
    pub fn abort(&self) {
        self.abort.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}
