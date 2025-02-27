use fframes::{AudioTimelineFrames, NamedRange, ResolvedAudioMap, ResolvedScenesTimeline};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen]
pub struct VideoMetadata {
    pub width: f64,
    pub height: f64,
    #[wasm_bindgen(js_name = durationInFrames)]
    pub duration: i32,
    pub fps: f64,
    #[wasm_bindgen(js_name = originalFps)]
    pub original_fps: Option<f64>,
    name: &'static str,
    audio_map: Option<Vec<NamedRange>>,
    scenes: Option<Vec<NamedRange>>,
}

impl VideoMetadata {
    pub fn new<TVideo: fframes::Video>(
        duration: i32,
        fps: usize,
        audio_map: Option<ResolvedAudioMap<AudioTimelineFrames>>,
        scenes: Option<&ResolvedScenesTimeline>,
    ) -> Self {
        Self {
            width: TVideo::WIDTH as f64,
            height: TVideo::HEIGHT as f64,
            duration,
            fps: fps as f64,
            original_fps: (fps != TVideo::FPS).then_some(TVideo::FPS as f64),
            name: std::any::type_name::<TVideo>(),
            audio_map: audio_map.map(Into::into),
            scenes: scenes.map(Into::into),
        }
    }
}

#[wasm_bindgen]
/// Can not be pulled out of macro because wasm_bindgen does not work with generics
impl VideoMetadata {
    #[wasm_bindgen(getter, js_name=hasAudio)]
    pub fn has_audio(&self) -> bool {
        self.audio_map.is_some()
    }

    #[wasm_bindgen(getter, js_name = audioMap)]
    /// Returns a resolved audio_map with audio timestamps converted to frames.
    /// The audio timestamp fallbacks to 0 if audio not loaded yet
    /// @returns {Record<string, [number, number]>}
    pub fn audio_map(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.audio_map).unwrap()
    }

    #[wasm_bindgen(getter, js_name = scenesTimeline)]
    /// Returns a resolved audio_map with audio timestamps converted to frames.
    /// The audio timestamp fallbacks to 0 if audio not loaded yet
    /// @returns {Record<string, [number, number]>}
    pub fn scenes(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.scenes).unwrap()
    }

    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.to_string()
    }
}
