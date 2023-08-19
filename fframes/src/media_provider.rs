use crate::media::ImageData;
use crate::{error::Result, media, AudioData, FontFace, FontStretch, FontStyle};
use std::{collections::HashMap, fmt::Debug};

pub trait StaticMediaProvider: std::fmt::Debug + Sized {
    fn prepare() -> Result<Self>;
}

pub trait MediaProvider: Send + Sync + Debug {
    fn resolve_audio(&self, name: &str) -> Option<AudioData>;
    fn resolve_image(&self, name: &str) -> Option<ImageData>;
    fn resolve_subtitles(&self, name: &str) -> Option<&media::Subtitles>;
    fn resolve_font<'a>(
        &'a self,
        font_name: &str,
        font_weight: u16,
        font_style: FontStyle,
        font_stretch: FontStretch,
    ) -> Option<Box<dyn FontFace + 'a>>;
}

#[derive(Clone, Default, Debug)]
pub struct DynamicMediaProvider<'media> {
    pub audio: HashMap<String, crate::media::PreloadedAudioData>,
    pub images: HashMap<String, ImageData>,
    pub subtitles: HashMap<String, crate::media::Subtitles<'media>>,
}

impl MediaProvider for DynamicMediaProvider<'_> {
    fn resolve_audio(&self, name: &str) -> Option<AudioData> {
        todo!()
    }

    fn resolve_image(&self, name: &str) -> Option<ImageData> {
        self.images.get(name).cloned()
    }

    fn resolve_subtitles(&self, name: &str) -> Option<&media::Subtitles> {
        self.subtitles.get(name)
    }

    fn resolve_font<'a>(
        &'a self,
        font_name: &str,
        font_weight: u16,
        font_style: FontStyle,
        font_stretch: FontStretch,
    ) -> Option<Box<dyn FontFace + 'a>> {
        todo!()
    }
}
