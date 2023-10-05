use fframes::error::FFramesError;
use fframes::{
    AudioTimelineFrames, Duration, ResolvedAudioMap, ResolvedRenderingTimeline,
    ResolvedScenesTimeline, Scenes, ScenesWithAudio, StaticMediaProvider, TimeBase, Video,
};
use futures::future::join_all;
use std::collections::HashMap;
use std::ops::Deref;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

#[wasm_bindgen(module = "fframes-editor")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn load_audio_wasm_callback(path: &str) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
pub struct AudioData {
    pub sample_rate: i32,
    name: &'static str,
    mono_pcm_data: js_sys::Int16Array,
    fltp_data: js_sys::Float32Array,
}

impl AudioData {
    pub fn new(audio: &fframes::AudioData, name: &'static str) -> Self {
        let preloaded_data = match audio {
            fframes::AudioData::Preloaded(data) => data,
            fframes::AudioData::Lazy => panic!("Lazy audio is not supported in wasm"),
        };

        let fltp_data = preloaded_data
            .samples
            .iter()
            .map(|sample| *sample as f32 / i16::MAX as f32)
            .collect::<Vec<f32>>();

        Self {
            name,
            fltp_data: js_sys::Float32Array::from(fltp_data.as_slice()),
            mono_pcm_data: js_sys::Int16Array::from(preloaded_data.samples.deref()),
            sample_rate: preloaded_data.sample_rate,
        }
    }
}

#[wasm_bindgen]
impl AudioData {
    /// An expensive cloning of the whole audio buffer, should be called once per
    /// the app lifetime.
    #[wasm_bindgen(getter)]
    pub fn fltp_data(&self) -> js_sys::Float32Array {
        self.fltp_data.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn mono_pcm_data(&self) -> js_sys::Int16Array {
        self.mono_pcm_data.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.to_string()
    }
}

async fn resolve_used_audio_durations<'a, 'media: 'a, TStaticMedia: StaticMediaProvider<'media>>(
    tb: &'a TimeBase,
    duration: &'a Duration<'a>,
    scene_audios: &'a ScenesWithAudio<'a>,
    global_audio_map: &'a fframes::AudioMap<'a>,
    static_media: &'media TStaticMedia,
) -> fframes::error::Result<HashMap<&'a str, usize>> {
    let mut duration_map: HashMap<&'a str, usize> = HashMap::new();
    let mut all_used_files = global_audio_map.used_audio_files().unwrap_or_default();

    if let Some(mut duration_audios) = duration.used_audio_files() {
        all_used_files.append(&mut duration_audios);
    }
    if let Some(mut duration_audios) = scene_audios.used_audio_files() {
        all_used_files.append(&mut duration_audios);
    }

    if all_used_files.is_empty() {
        return Ok(HashMap::new());
    }

    // make sure that vec::dedup removes only consecutive duplicates
    all_used_files.sort();
    all_used_files.dedup();

    let all_used_files = all_used_files
        .into_iter()
        .filter_map(|file| {
            let static_audio = static_media.resolve_audio(file);

            // if audio is not static than we need to fetch and load it through the browser xhr
            match static_audio.map(|audio| {
                let duration = audio.duration_in_frames(tb);
                duration_map.insert(file, duration);
            }) {
                Some(_) => None,
                None => Some(file),
            }
        })
        .collect::<Vec<_>>();

    if all_used_files.is_empty() {
        return Ok(duration_map);
    }

    join_all(
        all_used_files
            .iter()
            .map(|file| load_audio_wasm_callback(file)),
    )
    .await
    .into_iter()
    .zip(all_used_files.into_iter())
    .try_for_each(|(js_value, file)| match js_value {
        Ok(val) => val
            .as_f64()
            .map(|val| {
                let frames = val * tb.fps as f64;
                duration_map.insert(file, frames as usize);
            })
            .ok_or_else(|| FFramesError::CanNotProcessAudioDuration(file.to_string())),
        Err(_) => Err(FFramesError::CanNotProcessAudioDuration(file.to_string())),
    })?;

    Ok(duration_map)
}

pub async fn prepare_video_with_audio<'a, TVideo: Video, TStaticMedia: StaticMediaProvider<'a>>(
    video: &'a TVideo,
    tb: &TimeBase,
    static_media: &'a TStaticMedia,
    scenes: &'a Scenes<'a>,
) -> (
    usize,
    Option<ResolvedScenesTimeline<'a>>,
    Option<ResolvedAudioMap<AudioTimelineFrames>>,
) {
    let video_duration = video.duration();

    let audio_map = video.audio();
    let scene_audios = ScenesWithAudio::new(&scenes);

    let audio_durations =
        resolve_used_audio_durations(tb, &video_duration, &scene_audios, &audio_map, static_media)
            .await
            .unwrap();

    let resolve_audio_duration_in_frames = |val: &str| {
        audio_durations
            .get(val)
            .copied()
            .ok_or_else(|| FFramesError::CanNotProcessAudioDuration(val.to_string()))
    };

    let ResolvedRenderingTimeline {
        duration_in_frames: duration,
        scenes,
        audio_map,
    } = fframes::resolve_timeline::<AudioTimelineFrames, _>(
        &video_duration,
        &scene_audios,
        tb,
        &audio_map,
        resolve_audio_duration_in_frames,
    )
    .unwrap();

    (duration, scenes, audio_map)
}
