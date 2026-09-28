use crate::media::{ImageData, Subtitles};
use crate::{AudioData, FontSource, Frame, MediaProvider, ResolvedScenesTimeline, Svgr};
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

        // Rounded to even sizes: yuv420 encoders reject odd dimensions.
        let even = |size: usize| (((size as f64 * scale) / 2.).round() as usize * 2).max(2);
        Self {
            width: even(width),
            height: even(height),
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
        let filename = filename.as_ref();
        let audio = self.media_source.and_then(|m| m.resolve_audio(filename));
        if audio.is_none() {
            crate::diagnostics::report_missing_media(
                crate::diagnostics::MediaKind::Audio,
                filename,
            );
        }
        audio
    }

    pub fn get_subtitles(&self, filename: impl AsRef<str>) -> Option<&'media Subtitles<'media>> {
        let filename = filename.as_ref();
        let subtitles = self
            .media_source
            .and_then(|m| m.resolve_subtitles(filename));
        if subtitles.is_none() {
            crate::diagnostics::report_missing_media(
                crate::diagnostics::MediaKind::Subtitles,
                filename,
            );
        }
        subtitles
    }

    pub fn get_image(&self, filename: impl AsRef<str>) -> Option<&'media ImageData<'media>> {
        let filename = filename.as_ref();
        let image = self.media_source.and_then(|m| m.resolve_image(filename));
        if image.is_none() {
            crate::diagnostics::report_missing_media(
                crate::diagnostics::MediaKind::Image,
                filename,
            );
        }
        image
    }

    pub fn get_video(&self, filename: impl AsRef<str>) -> Option<&'media VideoMedia> {
        let filename = filename.as_ref();
        let video = self.media_source.and_then(|m| m.resolve_video(filename));
        if video.is_none() {
            crate::diagnostics::report_missing_media(
                crate::diagnostics::MediaKind::Video,
                filename,
            );
        }
        video
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
