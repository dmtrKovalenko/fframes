use crate::{audio_data, subtitles};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ImageData {
    pub link: String,
    pub base64: Option<String>,
}

#[derive(Clone, Default, Debug)]
pub struct MediaProvider {
    pub audio: HashMap<String, audio_data::AudioData>,
    pub images: HashMap<String, ImageData>,
    pub subtitles: HashMap<String, subtitles::Subtitles>,
}
