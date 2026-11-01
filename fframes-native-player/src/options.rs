use fframes::MediaProvider;

/// Which Skia backend draws the frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerBackend {
    /// Metal (if compiled in), then Vulkan (if compiled in), then the Skia CPU raster backend.
    #[default]
    Auto,
    #[cfg(feature = "metal")]
    Metal,
    #[cfg(feature = "vulkan")]
    Vulkan,
    /// Skia CPU raster backend. Works everywhere but is slow for 1080p and above.
    Cpu,
}

/// Options for [`crate::play`]. The media and font fields mirror `fframes::RenderOptions`
/// so the same video setup can be rendered or previewed.
#[derive(Debug, Clone)]
pub struct PlayerOptions<'a, 'media> {
    pub media: Option<&'media dyn MediaProvider<'media>>,
    /// See `RenderOptions::load_system_fonts`.
    pub load_system_fonts: bool,
    /// See `RenderOptions::default_font`.
    pub default_font: &'a str,
    /// Window title, defaults to "fframes".
    pub title: &'a str,
    /// Initial window size in logical pixels. Landscape defaults fit within 1280×720.
    /// Portrait defaults fit up to 960 pixels tall with a timeline on the left, subject
    /// to the size of the display.
    pub window_size: Option<(u32, u32)>,
    pub backend: PlayerBackend,
    /// Decode source videos in software even when a hardware decoder is available.
    pub software_decode: bool,
    /// Wait for the display refresh to show a frame. With `false` a frame is shown as soon
    /// as it is drawn, up to the frame rate of the video: the picture can tear and the GPU
    /// stays busy. Only the GPU backends wait for the display.
    pub vsync: bool,
    /// Start playing right away instead of paused on `start_frame`.
    pub autoplay: bool,
    /// Restart from the first frame after the last one (toggle with `r`).
    pub looping: bool,
    pub start_frame: usize,
    /// Play the video's audio map on the default output device.
    pub audio: bool,
    /// Maximum number of threads building frame trees (`Video::render_frame`) in parallel.
    /// Defaults to half the available cores: up to four workers for video sources, which
    /// share one decoder per file, and up to eight otherwise. Source video workers are
    /// limited to the default buffer size plus one live frame.
    pub render_threads: Option<usize>,
    /// Maximum number of future frame trees to prepare. The default (`usize::MAX`)
    /// buffers up to four trees. Zero prepares only the currently due frame.
    /// Skia draws each tree directly into the window when it is due.
    pub prefetch_frames: usize,
    /// Master bus of the audio mix, see `RenderOptions::audio_mix`.
    pub audio_mix: fframes::AudioMixOptions,
}

impl Default for PlayerOptions<'_, '_> {
    fn default() -> Self {
        Self {
            media: None,
            load_system_fonts: false,
            default_font: "Arial",
            title: "fframes",
            window_size: None,
            backend: PlayerBackend::Auto,
            software_decode: false,
            vsync: true,
            autoplay: true,
            looping: true,
            start_frame: 0,
            audio: true,
            render_threads: None,
            prefetch_frames: usize::MAX,
            audio_mix: fframes::AudioMixOptions::default(),
        }
    }
}

impl PlayerOptions<'_, '_> {
    pub(crate) fn lookahead(&self) -> usize {
        if self.prefetch_frames == usize::MAX {
            crate::scheduler::MAX_STAGED_TREES
        } else {
            self.prefetch_frames
        }
    }

    pub(crate) fn render_threads(&self) -> usize {
        // Source decoders are shared and keep recent frames, so a few workers can
        // prepare trees in parallel. More workers than staged trees would idle.
        if self.media.is_some_and(MediaProvider::has_video_sources) {
            return self
                .render_threads
                .unwrap_or_else(|| (cores() / 2).min(crate::scheduler::MAX_STAGED_TREES))
                .clamp(1, crate::scheduler::MAX_STAGED_TREES + 1);
        }
        self.render_threads
            .unwrap_or_else(|| (cores() / 2).clamp(1, 8))
            .max(1)
    }
}

/// The preview owns its workers. Querying Rayon here starts a second idle pool.
fn cores() -> usize {
    std::env::var("FFRAMES_NUM_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            std::env::var("RAYON_NUM_THREADS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|&threads| threads > 0)
        })
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookahead_defaults_to_a_small_tree_buffer() {
        let mut options = PlayerOptions::default();
        assert_eq!(options.lookahead(), 4);
        options.prefetch_frames = 8;
        assert_eq!(options.lookahead(), 8);
        options.prefetch_frames = 0;
        assert_eq!(options.lookahead(), 0);
    }

    #[test]
    fn video_sources_default_to_a_few_workers() {
        let media = fframes::DynamicMediaProvider::default().with_video(
            "clip.mp4",
            fframes::media::VideoMedia {
                path: "clip.mp4".into(),
                metadata: None,
            },
        );
        let media = fframes::CombinedMediaProvider::from([&() as &dyn MediaProvider, &media]);
        let mut options = PlayerOptions {
            media: Some(&media),
            ..Default::default()
        };
        assert_eq!(
            options.render_threads(),
            (fframes::get_thread_count() / 2).clamp(1, 4)
        );
        options.render_threads = Some(4);
        assert_eq!(options.render_threads(), 4);
        options.render_threads = Some(16);
        assert_eq!(options.render_threads(), 5);
        options.render_threads = Some(0);
        assert_eq!(options.render_threads(), 1);
        options.media = None;
        options.render_threads = Some(16);
        assert_eq!(options.render_threads(), 16);
        options.render_threads = None;
        assert_eq!(
            options.render_threads(),
            (fframes::get_thread_count() / 2).clamp(1, 8)
        );
    }
}
