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
            static ref TIME_BASE: Mutex<Option<fframes::TimeBase>> = Mutex::new(None);
            static ref MEDIA_PROVIDER: Mutex<fframes::MediaProvider> =
                Mutex::new(fframes::MediaProvider {
                    audio: HashMap::new(),
                    images: HashMap::new(),
                    subtitles: HashMap::new(),
                });
        }

        #[wasm_bindgen]
        pub async fn prepare(fps: Option<i32>) -> Result<video_metadata::VideoMetadata, JsValue> {
            console_error_panic_hook::set_once();

            let tb = fframes::TimeBase {
                fps: fps.map(std::convert::TryInto::try_into).and_then(Result::ok).unwrap_or($x::FPS),
                sample_rate: 44100,
            };

            TIME_BASE.lock().unwrap().replace(tb);

            let (duration, scenes, audio_map) = wasm_audio_map::prepare_video_with_audio(&VIDEO, &tb).await;

            let mut duration_mutex_ref = DURATION_IN_FRAMES.lock().unwrap();
            *duration_mutex_ref = duration;

            let video_metadata = video_metadata::VideoMetadata::new::<$x>(
                duration as i32,
                tb.fps,
                audio_map,
                scenes.as_ref()
            );

            if let Some(scenes) = scenes {
                SCENES.lock().unwrap().replace(scenes);
            }

            Ok(video_metadata)
        }

        #[wasm_bindgen]
        pub fn add_audio_source(file: String, input: &[i16]) {
            let audio_data = fframes::AudioData::Preloaded(
                fframes::PreloadedAudioData {
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
                fframes::ImageData {
                    link: url,
                    base64: base64_data
                }
            );
        }

        #[wasm_bindgen]
        pub fn render_frame(frame: i64) -> String {
            use std::ops::Deref;
            let time_base = TIME_BASE.lock().unwrap().expect("TimeBase must be set up before rendering.");

            VIDEO.render_frame(
                Frame {
                    fps: time_base.fps,
                    index: frame as usize,
                    global_index: frame as usize,
                    breaks_lru_cache: Some(BREAK_LINES_CACHE.clone()),
                },
                &FFramesContext {
                    duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                    mode: FFramesMode::Editor,
                    time_base,
                    font_source: Some(FONTS.lock().unwrap().deref()),
                    scenes:  SCENES.lock().unwrap().as_ref(),
                    media_provider: MEDIA_PROVIDER.lock().unwrap().deref(),
                },
            ).value
        }

        #[wasm_bindgen]
        pub fn render_preview_frame(frame: i64) -> String {
            use std::ops::Deref;
            let time_base = TIME_BASE.lock().unwrap().expect("TimeBase must be set up before rendering.");

            VIDEO.render_frame(
                Frame {
                    fps: time_base.fps,
                    index: frame as usize,
                    global_index: frame as usize,
                    breaks_lru_cache: None.into(),
                },
                &FFramesContext {
                    duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                    mode: FFramesMode::EditorTimelinePreview,
                    time_base,
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

            serde_wasm_bindgen::to_value(&face_info).unwrap()
        }
    };
}
