use crate::{error::Result, media, AudioData, Video};
use std::{collections::HashMap, fmt::Debug};

pub trait MediaProvider<'a>: Send + Sync + Debug {
    fn resolve_audio(&'a self, name: &str) -> Option<&'a AudioData>;
    fn resolve_image(&'a self, name: &str) -> Option<&'a media::ImageData>;
    fn resolve_subtitles(&'a self, name: &str) -> Option<&'a media::Subtitles>;

    // this hooks mainly used from the editor and might be optimised for wasm usage
    fn get_all_font_data(&'a self) -> Vec<(&'a [u8], &'a str)>;
    fn get_all_audio_data(&'a self) -> Vec<(&'a AudioData, &'a str)>;
}

pub trait StaticMediaProvider<'a>: std::fmt::Debug + Sized + MediaProvider<'a> {
    fn prepare() -> Result<Self>;
}

pub trait VideoMaybeWithStaticMedia<'a, TMedia: StaticMediaProvider<'a>>: Video {
    fn media(&'a self) -> Option<&'a TMedia> {
        return None;
    }
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

    fn get_all_font_data(&'a self) -> Vec<(&'a [u8], &'a str)> {
        Vec::with_capacity(0)
    }

    fn get_all_audio_data(&'a self) -> Vec<(&'a AudioData, &'a str)> {
        Vec::with_capacity(0)
    }
}

impl StaticMediaProvider<'_> for () {
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

    fn get_all_font_data(&'a self) -> Vec<(&'a [u8], &'a str)> {
        Vec::with_capacity(0)
    }

    fn get_all_audio_data(&'a self) -> Vec<(&'a AudioData, &'a str)> {
        Vec::with_capacity(0)
    }
}

#[derive(Debug)]
/// Represents unlimited amount of media providers
pub struct MediaSource<'a, const N: usize>([&'a (dyn MediaProvider<'a> + 'a); N]);

impl<'a, T: MediaProvider<'a>> From<&'a T> for MediaSource<'a, 1> {
    fn from(provider: &'a T) -> Self {
        Self([provider as &dyn MediaProvider])
    }
}

impl<'a, const N: usize> MediaProvider<'a> for MediaSource<'a, N> {
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

    fn get_all_font_data(&self) -> Vec<(&'a [u8], &'a str)> {
        // self.0.iter().fold(Vec::new(), |mut acc, provider| {
        //     acc.extend(provider.get_all_font_data());
        //     acc
        // })
        todo!()
    }

    fn get_all_audio_data(&self) -> Vec<(&'a AudioData, &'a str)> {
        // self.0.iter().fold(Vec::new(), |mut acc, provider| {
        //     acc.extend(provider.get_all_audio_data());
        //     acc
        // })
        //
        todo!()
    }
}
