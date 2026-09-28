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
    /// Initial window size in logical pixels. Defaults to the video size, scaled down to
    /// fit 1280x720.
    pub window_size: Option<(u32, u32)>,
    pub backend: PlayerBackend,
    /// Start playing right away instead of paused on `start_frame`.
    pub autoplay: bool,
    /// Restart from the first frame after the last one (toggle with `r`).
    pub looping: bool,
    pub start_frame: usize,
    /// Play the video's audio map on the default output device.
    pub audio: bool,
    /// Threads building frame trees (`Video::render_frame`) in parallel.
    /// Defaults to half of the available cores, at most 8.
    pub render_threads: Option<usize>,
    /// How many frames ahead of the playhead may be prepared.
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
            autoplay: true,
            looping: true,
            start_frame: 0,
            audio: true,
            render_threads: None,
            prefetch_frames: 12,
            audio_mix: Default::default(),
        }
    }
}

impl PlayerOptions<'_, '_> {
    pub(crate) fn render_threads(&self) -> usize {
        self.render_threads
            .unwrap_or_else(|| (fframes::get_thread_count() / 2).clamp(1, 8))
            .max(1)
    }
}
