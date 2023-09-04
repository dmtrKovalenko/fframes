use crate::{error::Result, media, AudioData};
use std::{collections::HashMap, fmt::Debug};

pub trait MediaProvider: Send + Sync + Debug {
    fn resolve_audio(&self, name: &str) -> Option<&AudioData>;
    fn resolve_image(&self, name: &str) -> Option<&media::ImageData>;
    fn resolve_subtitles(&self, name: &str) -> Option<&media::Subtitles>;

    // this hooks mainly used from the editor and might be optimised for wasm usage
    fn get_all_font_data(&self) -> Vec<&[u8]>;
    fn get_all_audio_data(&self) -> Vec<&AudioData>;
}

pub trait StaticMediaProvider: std::fmt::Debug + Sized + MediaProvider {
    fn prepare() -> Result<Self>;
}

impl MediaProvider for () {
    fn resolve_audio(&self, _name: &str) -> Option<&AudioData> {
        None
    }

    fn resolve_image(&self, _name: &str) -> Option<&media::ImageData> {
        None
    }

    fn resolve_subtitles(&self, _name: &str) -> Option<&media::Subtitles> {
        None
    }

    fn get_all_font_data(&self) -> Vec<&[u8]> {
        Vec::with_capacity(0)
    }

    fn get_all_audio_data(&self) -> Vec<&AudioData> {
        Vec::with_capacity(0)
    }
}

impl StaticMediaProvider for () {
    fn prepare() -> Result<Self> {
        Ok(())
    }
}

#[derive(Clone, Default, Debug)]
pub struct DynamicMediaProvider<'media> {
    pub audio: HashMap<String, AudioData<'media>>,
    pub images: HashMap<String, crate::media::ImageData>,
    pub subtitles: HashMap<String, crate::media::Subtitles<'media>>,
    pub fontdata: Vec<&'media [u8]>,
}

impl<'media> DynamicMediaProvider<'media> {
    pub fn new(
        audio: HashMap<String, AudioData<'media>>,
        images: HashMap<String, crate::media::ImageData>,
        subtitles: HashMap<String, crate::media::Subtitles<'media>>,
    ) -> Self {
        Self {
            audio,
            images,
            subtitles,
            fontdata: Vec::new(),
        }
    }
}

impl MediaProvider for DynamicMediaProvider<'_> {
    fn resolve_audio(&self, name: &str) -> Option<&AudioData> {
        self.audio.get(name)
    }

    fn resolve_image(&self, name: &str) -> Option<&media::ImageData> {
        self.images.get(name)
    }

    fn resolve_subtitles(&self, name: &str) -> Option<&media::Subtitles> {
        self.subtitles.get(name)
    }

    fn get_all_font_data(&self) -> Vec<&[u8]> {
        self.fontdata.clone()
    }

    fn get_all_audio_data(&self) -> Vec<&AudioData> {
        self.audio.values().collect()
    }
}
