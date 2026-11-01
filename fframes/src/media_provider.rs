use crate::{AudioData, FontSource, error::Result, media};
use fframes_media::VideoMedia;
use std::{collections::HashMap, fmt::Debug, sync::Arc};

#[derive(Clone)]
pub struct RawFontData {
    pub file_name: String,
    pub data: Arc<dyn AsRef<[u8]> + Sync + Send>,
}

impl Debug for RawFontData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawFontData")
            .field("file_name", &self.file_name)
            .field("data", &self.data.as_ref().as_ref())
            .finish()
    }
}

pub trait MediaProvider<'a>: Send + Sync + Debug {
    fn resolve_audio(&'a self, name: &str) -> Option<&'a AudioData<'a>>;
    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData<'a>>;
    fn resolve_subtitles(&'a self, name: &str) -> Option<&'a media::Subtitles<'a>>;
    fn resolve_video(&'a self, name: &str) -> Option<&'a media::VideoMedia>;
    #[cfg(feature = "lottie")]
    fn resolve_lottie(&'a self, _name: &str) -> Option<&'a crate::Lottie> {
        None
    }

    /// Returns all the font data along with the original file name
    /// `fn populate_font_source(&'a self, font_source: &mut dyn FontSource) -> Result<Vec<RawFontData>>;`
    fn populate_font_source(&'a self, font_source: &mut dyn FontSource);
    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    );

    fn get_all_audio_data(&self) -> Vec<(&AudioData<'_>, &str)> {
        Vec::new()
    }

    fn get_all_font_data(&self) -> Vec<(&[u8], &str)> {
        Vec::new()
    }

    fn get_all_image_data(&self) -> Vec<(&media::ImageData<'_>, &str)> {
        Vec::new()
    }

    fn get_all_video_data(&self) -> Vec<(&media::VideoMedia, &str)> {
        Vec::new()
    }

    /// Whether the provider lists video files that need a decoder during playback.
    /// A provider that can return `Some` from `resolve_video` must override this method
    /// or `get_all_video_data`. Otherwise the native preview uses its no-video defaults:
    /// more workers, and decoders that are not shared between them.
    fn has_video_sources(&self) -> bool {
        !self.get_all_video_data().is_empty()
    }

    #[cfg(feature = "lottie")]
    fn get_all_lottie_data(&self) -> Vec<(&crate::Lottie, &str)> {
        Vec::new()
    }
}

pub trait StaticMediaProvider<'a>: std::fmt::Debug + Sized + MediaProvider<'a> {
    fn prepare() -> Result<Self>;
}

impl<'a> MediaProvider<'a> for () {
    fn resolve_audio(&self, _name: &str) -> Option<&'a AudioData<'_>> {
        None
    }

    fn resolve_image(&'a self, _name: &str) -> Option<&'a media::ImageData<'a>> {
        None
    }

    fn resolve_subtitles(&'a self, _name: &str) -> Option<&'a media::Subtitles<'a>> {
        None
    }

    fn resolve_video(&'a self, _name: &str) -> Option<&'a VideoMedia> {
        None
    }

    fn populate_font_source(&'a self, _font_source: &mut dyn FontSource) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        _image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    ) {
    }
}

impl StaticMediaProvider<'_> for () {
    fn prepare() -> Result<Self> {
        Ok(())
    }
}

/// Media loaded at runtime. Start with `DynamicMediaProvider::default()` and add files with the
/// `with_*` methods:
///
/// ```rust,ignore
/// let music = DynamicMediaProvider::default().with_audio("music", AudioData::Preloaded(samples));
/// ```
///
/// `Default` gives empty media of every kind that the enabled features support.
#[derive(Clone, Default, Debug)]
pub struct DynamicMediaProvider<'media> {
    pub audio: HashMap<String, AudioData<'media>>,
    pub images: HashMap<String, media::ImageData<'media>>,
    pub subtitles: HashMap<String, media::Subtitles<'media>>,
    pub videos: HashMap<String, media::VideoMedia>,
    pub fontdata: Vec<RawFontData>,
    #[cfg(feature = "lottie")]
    pub lottie: HashMap<String, crate::Lottie>,
}

impl<'media> DynamicMediaProvider<'media> {
    #[deprecated(note = "use `DynamicMediaProvider::default()` and the `with_*` methods")]
    pub fn new(
        audio: HashMap<String, AudioData<'media>>,
        images: HashMap<String, media::ImageData<'media>>,
        subtitles: HashMap<String, media::Subtitles<'media>>,
        videos: HashMap<String, media::VideoMedia>,
        fonts_data: Vec<RawFontData>,
    ) -> Self {
        Self {
            audio,
            images,
            subtitles,
            videos,
            fontdata: fonts_data,
            #[cfg(feature = "lottie")]
            lottie: HashMap::new(),
        }
    }

    /// Adds an audio track. `ctx.get_audio(name)` and the audio map find it by `name`.
    #[must_use]
    pub fn with_audio(mut self, name: impl Into<String>, audio: AudioData<'media>) -> Self {
        self.audio.insert(name.into(), audio);
        self
    }

    /// Adds an image. `ctx.get_image(name)` finds it by `name`.
    #[must_use]
    pub fn with_image(mut self, name: impl Into<String>, image: media::ImageData<'media>) -> Self {
        self.images.insert(name.into(), image);
        self
    }

    /// Adds subtitles. `ctx.get_subtitles(name)` finds them by `name`.
    #[must_use]
    pub fn with_subtitles(
        mut self,
        name: impl Into<String>,
        subtitles: media::Subtitles<'media>,
    ) -> Self {
        self.subtitles.insert(name.into(), subtitles);
        self
    }

    /// Adds a video. `ctx.get_video(name)` finds it by `name`.
    #[must_use]
    pub fn with_video(mut self, name: impl Into<String>, video: media::VideoMedia) -> Self {
        self.videos.insert(name.into(), video);
        self
    }

    /// Adds a font. Text finds it by its family name.
    #[must_use]
    pub fn with_font(mut self, font: RawFontData) -> Self {
        self.fontdata.push(font);
        self
    }

    /// Adds a Lottie animation. `frame.play_lottie(ctx, name, ..)` finds it by `name`.
    #[cfg(feature = "lottie")]
    #[must_use]
    pub fn with_lottie(mut self, name: impl Into<String>, lottie: crate::Lottie) -> Self {
        self.lottie.insert(name.into(), lottie);
        self
    }
}

impl<'a> MediaProvider<'a> for DynamicMediaProvider<'a> {
    fn resolve_audio(&self, name: &str) -> Option<&'a AudioData<'_>> {
        self.audio.get(name)
    }

    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData<'a>> {
        self.images.get(name)
    }

    fn resolve_subtitles(&'a self, name: &str) -> Option<&'a media::Subtitles<'a>> {
        self.subtitles.get(name)
    }

    fn resolve_video(&'a self, name: &str) -> Option<&'a media::VideoMedia> {
        self.videos.get(name)
    }

    #[cfg(feature = "lottie")]
    fn resolve_lottie(&'a self, name: &str) -> Option<&'a crate::Lottie> {
        self.lottie.get(name)
    }

    fn populate_font_source(&'a self, font_source: &mut dyn FontSource) {
        for font in &self.fontdata {
            font_source.add_font(font.file_name.clone(), font.data.clone());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    ) {
        for (name, data) in &self.images {
            image_data.insert(name.clone(), data.href());
        }
    }

    fn get_all_font_data(&self) -> Vec<(&[u8], &str)> {
        self.fontdata
            .iter()
            .map(|font| (font.data.as_ref().as_ref(), &font.file_name[..]))
            .collect()
    }

    fn get_all_audio_data(&self) -> Vec<(&AudioData<'_>, &str)> {
        self.audio
            .iter()
            .map(|(name, data)| (data, name.as_str()))
            .collect()
    }

    fn get_all_image_data(&self) -> Vec<(&media::ImageData<'_>, &str)> {
        self.images
            .iter()
            .map(|(name, data)| (data, name.as_str()))
            .collect()
    }

    fn has_video_sources(&self) -> bool {
        !self.videos.is_empty()
    }

    fn get_all_video_data(&self) -> Vec<(&fframes_media::VideoMedia, &str)> {
        self.videos
            .iter()
            .map(|(name, data)| (data, name.as_str()))
            .collect()
    }
}

/// Combines several allocated media providers into one.
/// Allows to combine static and dynamic media providers.
#[derive(Debug)]
pub struct CombinedMediaProvider<'a, const N: usize>([&'a (dyn MediaProvider<'a> + 'a); N]);

impl<'a, T: MediaProvider<'a>> From<&'a T> for CombinedMediaProvider<'a, 1> {
    fn from(provider: &'a T) -> Self {
        Self([provider as &dyn MediaProvider])
    }
}

impl<'a, const N: usize> From<[&'a (dyn MediaProvider<'a> + 'a); N]>
    for CombinedMediaProvider<'a, N>
{
    fn from(provider: [&'a (dyn MediaProvider<'a> + 'a); N]) -> Self {
        Self(provider)
    }
}

impl<'a, const N: usize> MediaProvider<'a> for CombinedMediaProvider<'a, N> {
    fn resolve_audio(&self, name: &str) -> Option<&'a AudioData<'_>> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_audio(name))
    }

    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData<'a>> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_image(name))
    }

    fn resolve_subtitles(&self, name: &str) -> Option<&'a media::Subtitles<'_>> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_subtitles(name))
    }

    fn resolve_video(&'a self, name: &str) -> Option<&'a media::VideoMedia> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_video(name))
    }

    fn has_video_sources(&self) -> bool {
        self.0.iter().any(|provider| provider.has_video_sources())
    }

    #[cfg(feature = "lottie")]
    fn resolve_lottie(&'a self, name: &str) -> Option<&'a crate::Lottie> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_lottie(name))
    }

    fn populate_font_source(&'a self, font_source: &mut dyn FontSource) {
        for provider in &self.0 {
            provider.populate_font_source(font_source);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    ) {
        for provider in &self.0 {
            provider.populate_image_source(image_data);
        }
    }
}
