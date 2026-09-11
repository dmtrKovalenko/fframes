use fframes::{FFramesContext, FFramesMode, Frame, StaticMediaProvider, Video, VideoSize};
use std::{cell::RefCell, collections::HashMap, marker::PhantomPinned, pin::Pin, sync::Mutex};
use wasm_bindgen::prelude::*;

use crate::{video_metadata, wasm_audio, wasm_font_source};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch)]
    fn __fframes_get_video_frame(
        filename: &str,
        pts: f64,
        preview: bool,
    ) -> Result<JsValue, JsValue>;
}

fn resolve_video_frame(filename: &str, pts: i64, preview: bool) -> Option<fframes::VideoFrameData> {
    // A JS exception must not unwind through the editor's `RefCell` borrows.
    let result = __fframes_get_video_frame(filename, pts as f64, preview).ok()?;
    if result.is_null() || result.is_undefined() {
        return None;
    }

    let url = js_sys::Reflect::get(&result, &"url".into())
        .ok()?
        .as_string()?;
    let width = js_sys::Reflect::get(&result, &"width".into())
        .ok()?
        .as_f64()? as u32;
    let height = js_sys::Reflect::get(&result, &"height".into())
        .ok()?
        .as_f64()? as u32;

    let preview_url = js_sys::Reflect::get(&result, &"posterUrl".into())
        .ok()
        .and_then(|v| v.as_string());

    Some(fframes::VideoFrameData {
        url,
        preview_url,
        width,
        height,
    })
}

struct VideoCtx<T: Video> {
    video: T,
    raw_scenes: fframes::Scenes<'static>,
    _marker: PhantomPinned,
}

impl<T: Video> VideoCtx<T> {
    fn new(video: T) -> Pin<Box<Self>> {
        let inner = VideoCtx {
            video,
            raw_scenes: fframes::Scenes::empty(),
            _marker: PhantomPinned,
        };

        let mut pinned = Box::pin(inner);

        // SAFETY: We're creating a self-reference, but we ensure the struct
        // cannot be moved by pinning it and using PhantomPinned
        unsafe {
            let self_ptr: &T = &pinned.as_ref().video;
            let scenes = self_ptr.define_scenes();
            let scenes =
                std::mem::transmute::<fframes::Scenes<'_>, fframes::Scenes<'static>>(scenes);

            Pin::get_unchecked_mut(Pin::as_mut(&mut pinned)).raw_scenes = scenes;
        }

        pinned
    }
}

pub struct WasmEditor<T: Video, TMedia: StaticMediaProvider<'static> + 'static> {
    video_ctx: RefCell<Pin<Box<VideoCtx<T>>>>,
    static_media: &'static TMedia,
    duration_in_frames: Mutex<usize>,
    break_lines_cache: fframes::TextCache,
    fonts: Mutex<wasm_font_source::WasmFontSource>,
    scenes: Mutex<Option<fframes::ResolvedScenesTimeline<'static>>>,
    time_base: Mutex<Option<fframes::TimeBase>>,
    media_provider: Mutex<fframes::DynamicMediaProvider<'static>>,
    video_decoders: fframes::VideoDecodersWorker,
}

impl<TVideo: Video, TMedia: StaticMediaProvider<'static> + 'static> WasmEditor<TVideo, TMedia> {
    pub fn new(video: TVideo, static_media: &'static TMedia) -> Self {
        fframes::set_video_frame_resolver(resolve_video_frame);
        Self {
            video_ctx: RefCell::new(VideoCtx::new(video)),
            static_media,
            duration_in_frames: Mutex::new(0),
            break_lines_cache: fframes::TextCache::new(20).unwrap(),
            fonts: Mutex::new(wasm_font_source::WasmFontSource::new()),
            scenes: Mutex::new(None),
            time_base: Mutex::new(None),
            media_provider: Mutex::new(fframes::DynamicMediaProvider::new(
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
                Vec::new(),
            )),
            video_decoders: fframes::VideoDecodersWorker::default(),
        }
    }

    pub async fn replace(&self, video: TVideo) -> Result<video_metadata::VideoMetadata, JsValue> {
        let fps = { self.time_base.lock().unwrap().as_ref().map(|tb| tb.fps) };

        self.video_ctx.replace(VideoCtx::new(video));
        self.prepare(fps).await
    }

    pub async fn prepare(
        &self,
        fps: Option<usize>,
    ) -> Result<video_metadata::VideoMetadata, JsValue> {
        let tb = fframes::TimeBase {
            fps: fps.unwrap_or(TVideo::FPS),
            sample_rate: 44100,
        };

        self.time_base.lock().unwrap().replace(tb);
        let video_ctx = self.video_ctx.borrow();

        let (duration, scenes, audio_map) = wasm_audio::prepare_video_with_audio(
            &video_ctx.video,
            &tb,
            self.static_media,
            &video_ctx.raw_scenes,
        )
        .await;

        let mut duration_mutex_ref = self.duration_in_frames.lock().unwrap();
        *duration_mutex_ref = duration;

        let video_metadata = video_metadata::VideoMetadata::new::<TVideo>(
            duration as i32,
            tb.fps,
            audio_map,
            scenes.as_ref(),
        );

        if let Some(scenes_data) = scenes {
            self.scenes.lock().unwrap().replace(scenes_data.clone());
        }

        Ok(video_metadata)
    }

    pub fn add_audio_source(&self, file: String, sample_rate: i32, input: &[f32]) {
        let audio_data = fframes::media::PreloadedAudioData {
            sample_rate: sample_rate as u32,
            // Can't guarantee the lifetime of the input slice
            samples: std::borrow::Cow::Owned(input.to_vec()),
        };

        let mut media_provider = self.media_provider.lock().unwrap();
        media_provider
            .audio
            .insert(file.clone(), fframes::AudioData::Preloaded(audio_data));
    }

    pub fn add_subtitles_source(&self, file: String, content: String) -> usize {
        use fframes::media::FFramesSubtitles;

        let parsed_subtitle = fframes::media::Subtitles::parse(&content).unwrap();
        let phrases_count = (&parsed_subtitle).cues_count();

        self.media_provider
            .lock()
            .unwrap()
            .subtitles
            .insert(file, parsed_subtitle);

        phrases_count
    }

    pub fn add_image_source(
        &self,
        file: String,
        url: String,
        width: u32,
        height: u32,
        base64_data: Option<String>,
    ) {
        let mut media_provider = self.media_provider.lock().unwrap();
        let base64_data = fframes::media::OwnedSharedString::new_owned(
            base64_data.unwrap_or_else(|| url.clone()),
        );

        media_provider.images.insert(
            file.clone(),
            fframes::media::ImageData::new_from_web_source(
                base64_data,
                url,
                file,
                fframes::media::ImageMetadata {
                    width: width as u32,
                    height: height as u32,
                },
            ),
        );
    }

    pub fn add_video_source_placeholder(
        &self,
        file: String,
        url: String,
        width: u32,
        height: u32,
        duration: f32,
    ) {
        let mut media_provider = self.media_provider.lock().unwrap();
        let metadata = fframes::media::GeneralVideoFileMetadata {
            width: width as u32,
            height: height as u32,
            duration,
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

    pub fn render_frame(&self, frame: i64) -> String {
        use std::ops::Deref;
        let time_base = self
            .time_base
            .lock()
            .unwrap()
            .expect("TimeBase must be set up before rendering.");

        self.video_ctx
            .borrow()
            .video
            .render_frame(
                Frame::__internal_make_for_renderer(
                    frame as usize,
                    frame as usize,
                    time_base.fps,
                    Some(self.break_lines_cache.clone()),
                    self.video_decoders.clone(),
                ),
                &FFramesContext {
                    duration_in_frames: *self.duration_in_frames.lock().unwrap(),
                    current_video_size: VideoSize {
                        width: TVideo::WIDTH,
                        height: TVideo::HEIGHT,
                    },
                    mode: FFramesMode::Editor,
                    time_base,
                    font_source: Some(self.fonts.lock().unwrap().deref()),
                    scenes: self.scenes.lock().unwrap().as_ref(),
                    media_source: Some(self.media_provider.lock().unwrap().deref()),
                    abort_signal: None,
                },
            )
            .to_string()
    }

    pub fn render_preview_frame(&self, frame: i64) -> String {
        use std::ops::Deref;
        let time_base = self
            .time_base
            .lock()
            .unwrap()
            .expect("TimeBase must be set up before rendering.");

        #[cfg(target_arch = "wasm32")]
        fframes::media::IS_PREVIEW_RENDERING.with(|force_base64| {
            force_base64.store(true, std::sync::atomic::Ordering::Relaxed);
        });

        let result = self
            .video_ctx
            .borrow()
            .video
            .render_frame(
                Frame::__internal_make_for_renderer(
                    frame as usize,
                    frame as usize,
                    time_base.fps as usize,
                    Some(self.break_lines_cache.clone()),
                    self.video_decoders.clone(),
                ),
                &FFramesContext {
                    time_base,
                    current_video_size: VideoSize {
                        width: TVideo::WIDTH,
                        height: TVideo::HEIGHT,
                    },
                    duration_in_frames: *self.duration_in_frames.lock().unwrap(),
                    mode: FFramesMode::EditorTimelinePreview,
                    scenes: self.scenes.lock().unwrap().as_ref(),
                    media_source: Some(self.media_provider.lock().unwrap().deref()),
                    font_source: Some(self.fonts.lock().unwrap().deref()),
                    abort_signal: None,
                },
            )
            .to_string();

        fframes::media::IS_PREVIEW_RENDERING.with(|force_base64| {
            force_base64.store(false, std::sync::atomic::Ordering::Relaxed);
        });

        result
    }

    pub fn ingest_font(&self, slice: &[u8]) -> JsValue {
        let mut fonts = self.fonts.lock().unwrap();
        let face_info = fonts.insert_font(slice.to_vec().into(), None);

        serde_wasm_bindgen::to_value(&face_info).unwrap()
    }

    pub fn populate_static_fonts_db_with_static_fonts(&self) {
        let mut font_wasm_db = self.fonts.lock().unwrap();

        for (font_data, filename) in self.static_media.get_all_font_data() {
            font_wasm_db.insert_font(std::borrow::Cow::Borrowed(font_data), Some(filename));
        }
    }

    pub fn get_static_font_data_by_index(
        &self,
        index: usize,
    ) -> Option<wasm_font_source::StaticFontFace> {
        let fonts = self.fonts.lock().unwrap();
        fonts.static_fonts.get(index).cloned()
    }

    pub fn get_static_audio_data_by_index(&self, index: usize) -> Option<wasm_audio::AudioData> {
        let audios = self.static_media.get_all_audio_data();

        audios
            .get(index)
            .map(|(static_audio, filename)| wasm_audio::AudioData::new(*static_audio, filename))
    }
}
