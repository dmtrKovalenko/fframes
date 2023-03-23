#[macro_export]
macro_rules! setup_wasm_editor {
    ($x:tt, $params:tt) => {
        #[wasm_bindgen(module = "fframes-editor")]
        extern "C" {
            #[wasm_bindgen(catch)]
            async fn load_audio_wasm_callback(path: &str) -> Result<JsValue, JsValue>;
        }

        static VIDEO: $x = $x $params;

        lazy_static! {
            static ref DURATION_IN_FRAMES: Mutex<usize> = Mutex::new(0);
            static ref BREAK_LINES_CACHE: fframes::BreaksLruCache = fframes::BreaksLruCache::new(10).unwrap();
            static ref FONTS: Mutex<wasm_font_source::WasmFontSource> = Mutex::new(wasm_font_source::WasmFontSource::new());
            static ref SCENES: Mutex<Option<fframes::ResolvedScenesTimeline>> = Mutex::new(None);
            static ref AUDIO_MAP: Mutex<Option<Vec<wasm_audio_map::AudioTrack>>> = Mutex::new(None);
            static ref TIME_BASE: Mutex<Option<fframes::TimeBase>> = Mutex::new(None);
            static ref MEDIA_PROVIDER: Mutex<fframes::media_provider::MediaProvider> =
                Mutex::new(fframes::media_provider::MediaProvider {
                    audio: HashMap::new(),
                    images: HashMap::new(),
                    subtitles: HashMap::new(),
                });
        }

        #[wasm_bindgen]
        pub struct VideoMetadata {
            duration: i32,
        }

        #[wasm_bindgen]
        /// Can not be pulled out of macro because wasm_bindgen does not work with generics
        impl VideoMetadata {
            #[wasm_bindgen(getter, js_name=durationInFrames)]
            pub fn duration_in_frames(&self) -> i32 {
                self.duration
            }

            #[wasm_bindgen(getter)]
            pub fn height(&self) -> f64 {
                $x::HEIGHT as f64
            }

            #[wasm_bindgen(getter)]
            pub fn width(&self) -> f64 {
                $x::WIDTH as f64
            }

            #[wasm_bindgen(getter = name)]
            pub fn name(&self) -> String {
                std::any::type_name::<$x>().to_owned()
            }

            #[wasm_bindgen(getter, js_name=hasAudio)]
            pub fn has_audio(&self) -> bool {
                AUDIO_MAP.lock().unwrap().is_some()
            }

            #[wasm_bindgen(getter, js_name = audioMap)]
            /// Returns a resolved audio_map with audio timestamps converted to frames.
            /// The audio timestamp fallbacks to 0 if audio not loaded yet
            /// @returns {Record<string, [number, number]>}
            pub fn audio_map(&self) -> JsValue {
                JsValue::from_serde(&AUDIO_MAP.lock().unwrap().clone()).unwrap()
            }

            #[wasm_bindgen(getter)]
            pub fn fps(&self) -> f64 {
                $x::FPS as f64
            }
        }

        #[wasm_bindgen]
        pub async fn prepare() -> Result<VideoMetadata, JsValue> {
            console_error_panic_hook::set_once();

            let tb = fframes::TimeBase {
                fps: $x::FPS,
                sample_rate: 44100,
            };

            let (duration, scenes, audio_map) = wasm_audio_map::prepare_video_with_audio(&VIDEO, &tb).await;

            if let Some(scenes) = scenes {
                SCENES.lock().unwrap().replace(scenes);
            }

            if let Some(audio_map) = audio_map {
                AUDIO_MAP.lock().unwrap().replace(audio_map);
            }

            let mut duration_mutex_ref =  DURATION_IN_FRAMES.lock().unwrap();
            *duration_mutex_ref = duration;

            TIME_BASE.lock().unwrap().replace(tb);

            Ok(VideoMetadata { duration: duration as i32 })
        }

        #[wasm_bindgen]
        pub fn add_audio_source(file: String, input: &[i16]) {
            let audio_data = fframes::audio_data::AudioData::Preloaded(
                fframes::audio_data::PreloadedAudioData {
                    sample_rate: 44100,
                    samples: input.to_vec(),
                },
            );

            let mut media_provider = MEDIA_PROVIDER.lock().unwrap();
            media_provider.audio.insert(file.clone(), audio_data);
        }

        #[wasm_bindgen]
        pub fn add_subtitles_source(file: String, content: String) -> usize {
            use std::str::FromStr;

            let parsed_subtitle = Subtitles::parse(content.as_str(), $x::FPS).unwrap();
            let phrases_count = parsed_subtitle.cues_count();

            let mut media_provider = MEDIA_PROVIDER
                .lock()
                .unwrap()
                .subtitles
                .insert(file, parsed_subtitle);

            phrases_count
        }

        #[wasm_bindgen]
        pub fn add_image_source(file: String, url: String, base64_data: Option<String>) {
            let mut media_provider = MEDIA_PROVIDER.lock().unwrap();

            media_provider.images.insert(
                file,
                fframes::media_provider::ImageData {
                    link: url,
                    base64: base64_data
                }
            );
        }

        #[wasm_bindgen]
        pub fn render_frame(frame: i64) -> String {
            use std::ops::Deref;

            VIDEO.render_frame(
                frame::Frame {
                    fps: $x::FPS,
                    index: frame as usize,
                    global_index: frame as usize,
                    breaks_lru_cache: Some(BREAK_LINES_CACHE.clone()),
                },
                &fframes_context::FFramesContext {
                    duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                    mode: fframes_context::FFramesMode::Editor,
                    time_base: TIME_BASE.lock().unwrap().expect("TimeBase must be set up before rendering."),
                    font_source: Some(FONTS.lock().unwrap().deref()),
                    scenes:  SCENES.lock().unwrap().as_ref(),
                    media_provider: MEDIA_PROVIDER.lock().unwrap().deref(),
                },
            ).value
        }

        #[wasm_bindgen]
        pub fn render_preview_frame(frame: i64) -> String {
            use std::ops::Deref;

            VIDEO.render_frame(
                frame::Frame {
                    fps: $x::FPS,
                    index: frame as usize,
                    global_index: frame as usize,
                    breaks_lru_cache: None.into(),
                },
                &fframes_context::FFramesContext {
                    duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                    mode: fframes_context::FFramesMode::EditorTimelinePreview,
                    time_base: TIME_BASE.lock().unwrap().expect("TimeBase must be set up before rendering."),
                    scenes:  SCENES.lock().unwrap().as_ref(),
                    media_provider: MEDIA_PROVIDER.lock().unwrap().deref(),
                    font_source: Some(FONTS.lock().unwrap().deref()),
                },
            ).value
        }

        #[wasm_bindgen]
        pub fn ingest_font(slice: &[u8]) -> JsValue {
            use fframes::FontSource;

            let mut fonts = FONTS.lock().unwrap();
            let face_info = fonts.insert_font(slice.to_vec());

            JsValue::from_serde(&face_info).unwrap()
        }
    };
}
