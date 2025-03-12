#[macro_export]
macro_rules! setup_wasm_editor {
    ($x:tt, $params:expr) => {
        setup_wasm_editor!($x, $params, ());
    };
    ($x:tt, $params:expr, $static_media:expr) => {
        #[wasm_bindgen(module = "@fframes/editor")]
        extern "C" {
            #[wasm_bindgen(catch)]
            async fn load_audio_wasm_callback(path: &str) -> Result<JsValue, JsValue>;
        }

        lazy_static! {
            static ref VIDEO: $x<'static> = $params;
            static ref RAW_SCENES: fframes::Scenes<'static> = VIDEO.define_scenes();
            static ref DURATION_IN_FRAMES: Mutex<usize> = Mutex::new(0);
            static ref BREAK_LINES_CACHE: fframes::BreaksLruCache =
                fframes::BreaksLruCache::new(10).unwrap();
            static ref FONTS: Mutex<wasm_font_source::WasmFontSource> =
                Mutex::new(wasm_font_source::WasmFontSource::new());
            static ref SCENES: Mutex<Option<fframes::ResolvedScenesTimeline<'static>>> =
                Mutex::new(None);
            static ref TIME_BASE: Mutex<Option<fframes::TimeBase>> = Mutex::new(None);
            static ref MEDIA_PROVIDER: Mutex<fframes::DynamicMediaProvider<'static>> =
                Mutex::new(fframes::DynamicMediaProvider::new(
                    HashMap::new(),
                    HashMap::new(),
                    HashMap::new(),
                    HashMap::new(),
                    Vec::new()
                ));
            static ref VIDEO_DECODERS: fframes::VideoDecodersWorker =
                fframes::VideoDecodersWorker::default();
        }

        #[wasm_bindgen]
        pub async fn prepare(fps: Option<i32>) -> Result<video_metadata::VideoMetadata, JsValue> {
            console_error_panic_hook::set_once();

            let tb = fframes::TimeBase {
                fps: fps
                    .map(std::convert::TryInto::try_into)
                    .and_then(Result::ok)
                    .unwrap_or($x::FPS),
                sample_rate: 44100,
            };

            TIME_BASE.lock().unwrap().replace(tb);
            let (duration, scenes, audio_map) =
                wasm_audio::prepare_video_with_audio(&*VIDEO, &tb, &$static_media, &RAW_SCENES)
                    .await;

            let mut duration_mutex_ref = DURATION_IN_FRAMES.lock().unwrap();
            *duration_mutex_ref = duration;

            let video_metadata = video_metadata::VideoMetadata::new::<$x>(
                duration as i32,
                tb.fps,
                audio_map,
                scenes.as_ref(),
            );

            if let Some(scenes) = scenes {
                SCENES.lock().unwrap().replace(scenes);
            }

            Ok(video_metadata)
        }

        #[wasm_bindgen]
        pub fn add_audio_source(file: String, sample_rate: i32, input: &[f32]) {
            let audio_data = fframes::media::PreloadedAudioData {
                sample_rate: sample_rate as u32,
                // Can't guarantee the lifetime of the input slice
                samples: std::borrow::Cow::Owned(input.to_vec()),
            };

            let mut media_provider = MEDIA_PROVIDER.lock().unwrap();
            media_provider
                .audio
                .insert(file.clone(), fframes::AudioData::Preloaded(audio_data));
        }

        #[wasm_bindgen]
        pub fn add_subtitles_source(file: String, content: String) -> usize {
            use fframes::media::FFramesSubtitles;

            let parsed_subtitle = fframes::media::Subtitles::parse(&content).unwrap();
            let phrases_count = (&parsed_subtitle).cues_count();

            let mut media_provider = MEDIA_PROVIDER
                .lock()
                .unwrap()
                .subtitles
                .insert(file, parsed_subtitle);

            phrases_count
        }

        #[wasm_bindgen]
        pub fn add_image_source(
            file: String,
            url: String,
            width: u32,
            height: u32,
            base64_data: Option<String>,
        ) {
            let mut media_provider = MEDIA_PROVIDER.lock().unwrap();
            let base64_data = fframes::media::Base64ImageData::new_owned(
                base64_data.unwrap_or_else(|| url.clone()),
            );

            media_provider.images.insert(
                file,
                fframes::media::ImageData::new_from_raw_data(
                    base64_data,
                    url,
                    fframes::media::ImageMetadata {
                        width: width as u32,
                        height: height as u32,
                    },
                ),
            );
        }

        #[wasm_bindgen]
        pub fn add_video_source_placeholder(
            file: String,
            url: String,
            width: u32,
            height: u32,
            duration: f32,
        ) {
            let mut media_provider = MEDIA_PROVIDER.lock().unwrap();
            let metadata = fframes::media::GeneralVideoFileMetadata {
                width: width as u32,
                height: height as u32,
                duration: duration,
                fps: 30.0, // hardcoding for user convenience
            };

            media_provider.videos.insert(
                file.clone(),
                fframes::media::VideoMedia {
                    metadata: Some(metadata),
                    path: std::path::PathBuf::from(url),
                },
            );
        }

        #[wasm_bindgen]
        pub fn render_frame(frame: i64) -> String {
            use std::ops::Deref;
            let time_base = TIME_BASE
                .lock()
                .unwrap()
                .expect("TimeBase must be set up before rendering.");

            VIDEO
                .render_frame(
                    Frame::__internal_make_for_renderer(
                        frame as usize,
                        frame as usize,
                        time_base.fps,
                        Some(BREAK_LINES_CACHE.clone()),
                        VIDEO_DECODERS.clone(),
                    ),
                    &FFramesContext {
                        duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                        current_video_size: VideoSize {
                            width: $x::WIDTH,
                            height: $x::HEIGHT,
                        },
                        mode: FFramesMode::Editor,
                        time_base,
                        font_source: Some(FONTS.lock().unwrap().deref()),
                        scenes: SCENES.lock().unwrap().as_ref(),
                        media_source: Some(MEDIA_PROVIDER.lock().unwrap().deref()),
                    },
                )
                .value
        }

        #[wasm_bindgen]
        pub fn render_preview_frame(frame: i64) -> String {
            use std::ops::Deref;
            let time_base = TIME_BASE
                .lock()
                .unwrap()
                .expect("TimeBase must be set up before rendering.");

            VIDEO
                .render_frame(
                    Frame::__internal_make_for_renderer(
                        frame as usize,
                        frame as usize,
                        time_base.fps as usize,
                        Some(BREAK_LINES_CACHE.clone()),
                        VIDEO_DECODERS.clone(),
                    ),
                    &FFramesContext {
                        time_base,
                        current_video_size: VideoSize {
                            width: $x::WIDTH,
                            height: $x::HEIGHT,
                        },
                        duration_in_frames: *DURATION_IN_FRAMES.lock().unwrap(),
                        mode: FFramesMode::EditorTimelinePreview,
                        scenes: SCENES.lock().unwrap().as_ref(),
                        media_source: Some(MEDIA_PROVIDER.lock().unwrap().deref()),
                        font_source: Some(FONTS.lock().unwrap().deref()),
                    },
                )
                .value
        }

        #[wasm_bindgen]
        pub fn ingest_font(slice: &[u8]) -> JsValue {
            use fframes::FontSource;

            let mut fonts = FONTS.lock().unwrap();
            let face_info = fonts.insert_font(slice.to_vec().into(), None);

            serde_wasm_bindgen::to_value(&face_info).unwrap()
        }

        #[wasm_bindgen]
        pub fn populate_static_fonts_db_with_static_fonts() {
            let mut font_wasm_db = FONTS.lock().unwrap();

            for (font_data, filename) in (&$static_media).get_all_font_data() {
                font_wasm_db.insert_font(font_data.into(), Some(filename));
            }
        }

        #[wasm_bindgen]
        pub fn get_static_font_data_by_index(
            index: usize,
        ) -> Option<wasm_font_source::StaticFontFace> {
            let fonts = FONTS.lock().unwrap();
            fonts.static_fonts.get(index).cloned()
        }

        #[wasm_bindgen]
        pub fn get_static_audio_data_by_index(index: usize) -> Option<wasm_audio::AudioData> {
            let audios = (&$static_media).get_all_audio_data();

            audios
                .get(index)
                .map(|(static_audio, filename)| wasm_audio::AudioData::new(*static_audio, filename))
        }
    };
}
