use crate::{error::Result, media, AudioData, FontSource};
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
    fn resolve_audio(&'a self, name: &str) -> Option<&'a AudioData>;
    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData>;
    fn resolve_subtitles(&'a self, name: &str) -> Option<&'a media::Subtitles>;

    /// Returns all the font data along with the original file name
    // fn populate_font_source(&'a self, font_source: &mut dyn FontSource) -> Result<Vec<RawFontData>>;
    fn populate_font_source(&'a self, font_source: &mut dyn FontSource);
    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    );
}

pub trait StaticMediaProvider<'a>: std::fmt::Debug + Sized + MediaProvider<'a> {
    fn prepare() -> Result<Self>;
    fn get_all_audio_data(&'a self) -> Option<Vec<(&'a AudioData, &'a str)>>;
    fn get_all_font_data(&'a self) -> Option<Vec<(&'a [u8], &'a str)>>;
}

impl<'a> MediaProvider<'a> for () {
    fn resolve_audio(&self, _name: &str) -> Option<&'a AudioData> {
        None
    }

    fn resolve_image(&'a self, _name: &str) -> Option<&'a media::ImageData> {
        None
    }

    fn resolve_subtitles(&'a self, _name: &str) -> Option<&'a media::Subtitles> {
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

    fn get_all_audio_data(&self) -> Option<Vec<(&AudioData, &str)>> {
        None
    }

    fn get_all_font_data(&self) -> Option<Vec<(&[u8], &str)>> {
        None
    }
}

#[derive(Clone, Default, Debug)]
pub struct DynamicMediaProvider<'media> {
    pub audio: HashMap<String, AudioData<'media>>,
    pub images: HashMap<String, crate::media::ImageData>,
    pub subtitles: HashMap<String, crate::media::Subtitles<'media>>,
    pub fontdata: Vec<RawFontData>,
}

impl<'media> DynamicMediaProvider<'media> {
    pub fn new(
        audio: HashMap<String, AudioData<'media>>,
        images: HashMap<String, crate::media::ImageData>,
        subtitles: HashMap<String, crate::media::Subtitles<'media>>,
        fonts_data: Vec<RawFontData>,
    ) -> Self {
        Self {
            audio,
            images,
            subtitles,
            fontdata: fonts_data,
        }
    }
}

impl<'a> MediaProvider<'a> for DynamicMediaProvider<'a> {
    fn resolve_audio(&self, name: &str) -> Option<&'a AudioData> {
        self.audio.get(name)
    }

    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData> {
        self.images.get(name)
    }

    fn resolve_subtitles(&'a self, name: &str) -> Option<&'a media::Subtitles> {
        self.subtitles.get(name)
    }

    fn populate_font_source(&'a self, font_source: &mut dyn FontSource) {
        for font in self.fontdata.iter() {
            font_source.add_font(font.file_name.clone(), font.data.clone())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    ) {
        for (name, data) in self.images.iter() {
            image_data.insert(name.clone(), data.image.clone());
        }
    }
}

#[derive(Debug)]
/// Represents unlimited amount of media source for the video
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
    fn resolve_audio(&self, name: &str) -> Option<&'a AudioData> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_audio(name))
    }

    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_image(name))
    }

    fn resolve_subtitles(&self, name: &str) -> Option<&'a media::Subtitles> {
        self.0
            .iter()
            .find_map(|provider| provider.resolve_subtitles(name))
    }

    fn populate_font_source(&'a self, font_source: &mut dyn FontSource) {
        for provider in self.0.iter() {
            provider.populate_font_source(font_source);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn populate_image_source(
        &'a self,
        image_data: &mut HashMap<String, Arc<usvgr::PreloadedImageData>>,
    ) {
        for provider in self.0.iter() {
            provider.populate_image_source(image_data);
        }
    }
}
