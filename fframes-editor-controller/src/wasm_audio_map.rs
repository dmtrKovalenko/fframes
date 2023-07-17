use fframes::error::FFramesCoreError;
use fframes::{
    AudioTimelineFrames, Duration, ResolvedAudioMap, ResolvedRenderingTimeline,
    ResolvedScenesTimeline, ScenesWithAudio, TimeBase, Video,
};
use futures::future::join_all;
use std::collections::HashMap;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

#[wasm_bindgen(module = "fframes-editor")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn load_audio_wasm_callback(path: &str) -> Result<JsValue, JsValue>;
}

async fn resolve_used_audio_durations<'a>(
    tb: &'a TimeBase,
    duration: &'a Duration<'a>,
    scene_audios: &'a ScenesWithAudio<'a>,
    global_audio_map: &'a fframes::AudioMap<'a>,
) -> fframes::error::Result<HashMap<&'a str, usize>> {
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

    join_all(
        all_used_files
            .iter()
            .map(|file| load_audio_wasm_callback(file)),
    )
    .await
    .into_iter()
    .zip(all_used_files.into_iter())
    .map(|(js_value, file)| match js_value {
        Ok(val) => val
            .as_f64()
            .map(|val| {
                let frames = val * tb.fps as f64;
                (file, frames as usize)
            })
            .ok_or_else(|| FFramesCoreError::CanNotProcessAudioDuration(file.to_string())),
        Err(_) => Err(FFramesCoreError::CanNotProcessAudioDuration(
            file.to_string(),
        )),
    })
    .collect::<fframes::error::Result<HashMap<_, _>>>()
}

pub async fn prepare_video_with_audio<TVideo: Video>(
    video: &TVideo,
    tb: &TimeBase,
) -> (
    usize,
    Option<ResolvedScenesTimeline>,
    Option<ResolvedAudioMap<AudioTimelineFrames>>,
) {
    let video_duration = video.duration();

    let audio_map = video.audio();
    let scenes = video.define_scenes();
    let scene_audios = ScenesWithAudio::from(&scenes);

    let audio_durations =
        resolve_used_audio_durations(tb, &video_duration, &scene_audios, &audio_map)
            .await
            .unwrap();

    let resolve_audio_duration_in_frames = |val: &str| {
        audio_durations
            .get(val)
            .copied()
            .ok_or_else(|| FFramesCoreError::CanNotProcessAudioDuration(val.to_string()))
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
