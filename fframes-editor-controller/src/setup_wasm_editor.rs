#[macro_export]
macro_rules! setup_wasm_editor {
    ($x:tt, $params:tt) => {
        #[wasm_bindgen(module = "fframes-editor")]
        extern "C" {
            #[wasm_bindgen(catch)]
            async fn load_audio_wasm_callback(path: &str) -> Result<JsValue, JsValue>;
        }

        static VIDEO: $x = $x $params;

        #[derive(Clone, fframes::serde::Serialize)]
        #[serde(crate = "fframes::serde")] // https://github.com/serde-rs/serde/issues/1465
        pub struct AudioTrack {
            pub name: String,
            pub start: usize,
            pub end: usize
        }

        lazy_static! {
            static ref DURATION_IN_FRAMES: Mutex<usize> = Mutex::new(0);
            static ref BREAK_LINES_CACHE: fframes::BreaksLruCache = fframes::BreaksLruCache::new(10).unwrap();
            static ref FONTS: Mutex<wasm_font_source::WasmFontSource> = Mutex::new(wasm_font_source::WasmFontSource::new());
            static ref SCENES: Mutex<Option<fframes::ResolvedScenesTimeline>> = Mutex::new(None);
            static ref AUDIO_MAP: Mutex<Option<Vec<AudioTrack>>> = {
                use fframes::AudioTimelineUnit;

                Mutex::new(
                    fframes::AudioMap::resolve::<fframes::AudioTimelineFrames>(
                        &$x::audio(&VIDEO),
                        &fframes_context::FFramesContext {
                            duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                            mode: fframes_context::FFramesMode::Editor,
                            fps: $x::FPS,
                            sample_rate: 44100,
                            scenes:  SCENES.lock().unwrap().as_ref(),
                            media_provider: MEDIA_PROVIDER.lock().unwrap().deref(),
                            font_source: None,
                        }
                    ).map(
                        |resolved_map| resolved_map.0.into_iter().map(|(name, range)| {
                            AudioTrack {
                                name,
                                start: range.start.as_usize(),
                                end: range.end.as_usize(),
                            }
                        })
                        .collect::<Vec<_>>()
                    )
                )
            };
            static ref AUDIO_DURATIONS: Mutex<HashMap<String, i32>> = Mutex::new(HashMap::new());
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
                $x::audio(&VIDEO).0.is_some()
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

        async fn load_audio_duration(audio: String) -> fframes::error::Result<usize> {
            let duration_in_frames = (load_audio_wasm_callback(audio.as_str())
                .await
                .expect("Can not get the duration of audio")
                .as_f64()
                .expect("Can not convert the duration of audio to f64")
                * $x::FPS as f64) as i32;

            let mut durations_hash = AUDIO_DURATIONS.lock().unwrap();
            durations_hash.insert(audio.to_owned(), duration_in_frames);

            Ok(duration_in_frames as usize)
        }

        async fn resolve_duration_and_scenes() -> i32 {
            let (duration, scenes) =
                fframes::resolve_duration_and_scenes_async(&VIDEO, |val| Box::pin(load_audio_duration(val)))
                .await
                .unwrap();

            if let Some(scenes) = scenes {
                SCENES.lock().unwrap().replace(scenes);
            }

            let mut duration_mutex_ref =  DURATION_IN_FRAMES.lock().unwrap();
            *duration_mutex_ref = duration;
            duration as i32
        }

        #[wasm_bindgen]
        pub async fn prepare() -> Result<VideoMetadata, JsValue> {
            console_error_panic_hook::set_once();

            let duration = resolve_duration_and_scenes().await;
            Ok(VideoMetadata { duration })
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

            let duration_in_frames = input.len() as f64 / 44100 as f64 * $x::FPS as f64;
            let mut durations_hash = AUDIO_DURATIONS.lock().unwrap();
            durations_hash.insert(file, duration_in_frames as i32);
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
                    fps: $x::FPS,
                    sample_rate: 44100,
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
                    fps: $x::FPS,
                    sample_rate: 44100,
                    scenes:  SCENES.lock().unwrap().as_ref(),
                    media_provider: MEDIA_PROVIDER.lock().unwrap().deref(),
                    font_source: None,
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
