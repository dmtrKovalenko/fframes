use fframes::error::FFramesError;
use fframes::{
    AudioTimelineFrames, Duration, ResolvedAudioMap, ResolvedRenderingTimeline,
    ResolvedScenesTimeline, Scenes, ScenesWithAudio, StaticMediaProvider, TimeBase, Video,
};
use futures::future::join_all;
use std::collections::HashMap;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(module = "@fframes/editor")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn load_audio_wasm_callback(path: &str) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
pub struct AudioData {
    pub sample_rate: u32,
    name: &'static str,
    fltp_data: js_sys::Float32Array,
}

impl AudioData {
    pub fn new(audio: &fframes::AudioData, name: &'static str) -> Self {
        let preloaded_data = match audio {
            fframes::AudioData::Preloaded(data) => data,
            fframes::AudioData::Lazy => panic!("Lazy audio is not supported in wasm"),
        };

        Self {
            name,
            fltp_data: js_sys::Float32Array::from(preloaded_data.samples.as_ref()),
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
    pub fn name(&self) -> String {
        self.name.to_string()
    }
}

/// Durations (in frames) of every media file the video and scene timelines
/// are resolved from.
///
/// Audio comes from static media or is decoded through the browser; video
/// durations come from the metadata the editor registered for the file
/// (`resolve_video_duration`, in seconds), falling back to the audio track
/// when the metadata is not registered yet.
async fn resolve_used_media_durations<'a, 'media: 'a, TStaticMedia: StaticMediaProvider<'media>>(
    tb: &TimeBase,
    duration: &'a Duration<'a>,
    scene_audios: &'a ScenesWithAudio<'media>,
    global_audio_map: &'a fframes::AudioMap<'a>,
    static_media: &'media TStaticMedia,
    resolve_video_duration: impl Fn(&str) -> Option<f32>,
) -> fframes::error::Result<HashMap<String, usize>> {
    let mut duration_map: HashMap<String, usize> = HashMap::new();

    let mut all_used_files = global_audio_map
        .used_audio_files::<Vec<&str>>()
        .unwrap_or_default();

    if let Some(duration_audios) = duration.used_audio_files() {
        all_used_files.extend(duration_audios.into_iter());
    }
    if let Some(duration_audios) = scene_audios.used_audio_files() {
        all_used_files.extend(duration_audios.into_iter());
    }

    // The editor registers video metadata while `prepare` is already
    // running, so a video that is not registered yet is measured through
    // its audio track instead (like any audio file).
    let mut video_durations: HashMap<String, usize> = HashMap::new();
    for file in duration
        .used_video_files()
        .into_iter()
        .chain(scene_audios.used_video_files())
        .flatten()
    {
        match resolve_video_duration(file) {
            Some(seconds) => {
                video_durations
                    .insert(file.to_string(), (seconds * tb.fps as f32).round() as usize);
            }
            None => all_used_files.push(file),
        }
    }

    if all_used_files.is_empty() {
        return Ok(video_durations);
    }

    all_used_files.sort_unstable();
    all_used_files.dedup();

    let all_used_files = all_used_files
        .into_iter()
        .filter(|file| {
            let static_audio = static_media.resolve_audio(file);

            // if audio is not static than we need to fetch and load it through the browser xhr
            static_audio
                .map(|audio| {
                    let duration = audio.duration_in_frames(tb);
                    duration_map.insert(file.to_string(), duration);
                })
                .is_none()
        })
        .collect::<Vec<_>>();

    if all_used_files.is_empty() {
        duration_map.extend(video_durations);
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
                duration_map.insert(file.to_string(), frames as usize);
            })
            .ok_or_else(|| FFramesError::CanNotProcessAudioDuration(file.to_string())),
        Err(_) => Err(FFramesError::CanNotProcessAudioDuration(file.to_string())),
    })?;

    // A video file's own duration wins over its audio track, as in the renderer.
    duration_map.extend(video_durations);
    Ok(duration_map)
}

pub async fn prepare_video_with_audio<
    'a,
    TVideo: Video,
    TStaticMedia: StaticMediaProvider<'static>,
>(
    video: &TVideo,
    tb: &TimeBase,
    static_media: &'static TStaticMedia,
    scenes: &Scenes<'static>,
    resolve_video_duration: impl Fn(&str) -> Option<f32>,
) -> (
    usize,
    Option<ResolvedScenesTimeline<'static>>,
    Option<ResolvedAudioMap<AudioTimelineFrames>>,
) {
    let video_duration = video.duration();

    let audio_map = video.audio();
    let scene_audios = ScenesWithAudio::new(scenes);

    let audio_durations = resolve_used_media_durations(
        tb,
        &video_duration,
        &scene_audios,
        &audio_map,
        static_media,
        resolve_video_duration,
    )
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
