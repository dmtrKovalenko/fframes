use crate::{FFramesContext, media::GeneralVideoFileMetadata, media::ImageData};
use fframes_media::VideoMedia;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
#[cfg(not(target_arch = "wasm32"))]
use fframes_media::ImageMetadata;
pub use fframes_media::{FrameConvertOptions, PlaybackSpeed, ResizeVideoFrame};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(target_arch = "wasm32")]
use std::cell::Cell;

/// A decoded frame of a video file.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type SourceFrame<'media> = Arc<crate::media::FFmpegFrameBuf>;
/// A decoded frame of a video file: the editor's frame or the fallback image.
#[cfg(target_arch = "wasm32")]
pub(crate) type SourceFrame<'media> = Arc<WasmEditorVideoFrameFallback<'media>>;

/// Data returned by the video frame resolver callback.
#[cfg(target_arch = "wasm32")]
pub struct VideoFrameData {
    pub url: String,
    /// Data URL fallback for timeline preview rendering (works inside SVG-as-image)
    pub preview_url: Option<String>,
    pub width: u32,
    pub height: u32,
}

/// Function pointer type for resolving video frames from JS.
/// Called synchronously during render_frame to get a decoded frame.
/// The last argument tells whether the frame is requested for a timeline
/// preview (which only needs the poster and must not steer the decode queue).
#[cfg(target_arch = "wasm32")]
pub type VideoFrameResolver = fn(&str, i64, bool) -> Option<VideoFrameData>;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static VIDEO_FRAME_RESOLVER: Cell<Option<VideoFrameResolver>> = Cell::new(None);
}

/// Set the video frame resolver callback. Called once during editor setup.
/// The resolver is invoked synchronously from WASM during render_frame.
#[cfg(target_arch = "wasm32")]
pub fn set_video_frame_resolver(resolver: VideoFrameResolver) {
    VIDEO_FRAME_RESOLVER.with(|r| r.set(Some(resolver)));
}

pub trait FFramesSyncedVideoFrame<'media> {
    /// Converts the frame to RGBA at its display dimensions.
    /// Native renderers share the pixels between consecutive requests at the same size.
    #[allow(clippy::wrong_self_convention)]
    fn into_image(&self) -> ImageData<'media>;
    /// Converts the frame to RGBA at the requested size.
    #[allow(clippy::wrong_self_convention)]
    fn into_resized_image(&self, options: &FrameConvertOptions) -> Option<ImageData<'media>>;
    /// Get the height of the frame with non-square pixels stretched to square ones
    fn height(&self) -> u32;
    /// Get the width of the frame with non-square pixels stretched to square ones
    fn width(&self) -> u32;
    /// Get the frame index of the origin video
    fn timestamp_seconds(&self) -> f32;
    /// Returns the native frame timestamp in the decoders' timebase
    fn timestamp(&self) -> i64;
    /// Get the FPS of the origin video
    fn stream_fps(&self) -> f32;
    /// Get the duration of the origin video in frames
    fn stream_duration_in_seconds(&self) -> f32;
}

#[derive(Debug, Clone, Copy)]
pub struct SyncVideoFrameInput<'media> {
    /// The second of the video (or scene) when the file starts playing from its beginning.
    /// It is not a position in the file: to play a part of a file use [`Clip`](crate::Clip).
    pub start_from: f32,
    /// If `true` the video playback will be looped and will start from the beginning on the end.
    /// Also if `true` the `get_synced_video_frame` API should never return None, only in case of
    /// decoding errors, so in some cases it might be okay to panic if the frame is not available.
    pub looping: bool,
    /// Playback speed of the video: `2.0` plays it twice as fast, `0.5` at half speed.
    /// Every frame shows the source frame at `seconds since start_from * speed`, so the
    /// motion stays even at any speed and frame rate. With `looping` a loop takes
    /// `duration / speed` seconds. The speed is kept to 1/1000 (`PlaybackSpeed`); a value
    /// that is not positive and finite plays at normal speed. Default `1.0`.
    /// The speed is constant: each speed of a file has its own decoder, and the
    /// renderer keeps the 4 used last.
    pub speed: f32,
    /// The wasm preview editor is not supporting decoding video frames in runtime
    /// so this allows to provide a fallback image for the debugging
    pub editor_fallback_image: Option<&'media ImageData<'media>>,
}

impl Default for SyncVideoFrameInput<'_> {
    fn default() -> Self {
        Self {
            start_from: 0.0,
            looping: false,
            speed: 1.0,
            editor_fallback_image: None,
        }
    }
}

/// This is used by the editor as a fallback for the static image
#[derive(Debug, Clone)]
pub struct WasmEditorVideoFrameFallback<'media> {
    fallback_image: ImageData<'media>,
    video_metadata: GeneralVideoFileMetadata,
}

impl<'media> FFramesSyncedVideoFrame<'media> for WasmEditorVideoFrameFallback<'media> {
    fn into_image(&self) -> ImageData<'media> {
        self.fallback_image.clone()
    }

    fn into_resized_image(&self, _options: &FrameConvertOptions) -> Option<ImageData<'media>> {
        Some(self.fallback_image.clone())
    }

    fn height(&self) -> u32 {
        self.video_metadata.height
    }

    fn width(&self) -> u32 {
        self.video_metadata.width
    }

    fn timestamp_seconds(&self) -> f32 {
        unimplemented!(
            "Timestamp seconds are not supported in the editor. Please mock this data for editing."
        )
    }

    fn timestamp(&self) -> i64 {
        unimplemented!(
            "Timestamp seconds are not supported in the editor. Please mock this data for editing."
        )
    }

    fn stream_fps(&self) -> f32 {
        self.video_metadata.fps
    }

    fn stream_duration_in_seconds(&self) -> f32 {
        self.video_metadata.duration
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn decoded_image<'media>(pixels: Arc<usvgr::PreloadedImageData>) -> ImageData<'media> {
    let metadata = ImageMetadata {
        width: pixels.width,
        height: pixels.height,
    };
    let id = pixels.id.clone();
    ImageData::new_from_raw_data(pixels, id, metadata)
}

#[cfg(not(target_arch = "wasm32"))]
impl<'media> FFramesSyncedVideoFrame<'media> for crate::media::FFmpegFrameBuf {
    fn into_image(&self) -> ImageData<'media> {
        let image_data = unsafe { self.convert_last_decoded_frame_into_svg_image(None) }
            .expect("Failed to convert frame to image");

        decoded_image(image_data)
    }

    fn into_resized_image(&self, options: &FrameConvertOptions) -> Option<ImageData<'media>> {
        let image_data = unsafe { self.convert_last_decoded_frame_into_svg_image(Some(options)) }
            .map_err(|e| {
                eprintln!("Error converting frame to image: {e:?}");
            })
            .ok()?;

        Some(decoded_image(image_data))
    }

    fn width(&self) -> u32 {
        self.get_stream_width()
    }

    fn height(&self) -> u32 {
        self.get_stream_height()
    }

    fn timestamp_seconds(&self) -> f32 {
        unsafe { self.timestamp_seconds() }
    }

    fn timestamp(&self) -> i64 {
        unsafe { self.get_pts() }
    }

    fn stream_duration_in_seconds(&self) -> f32 {
        unsafe { self.get_stream_duration_in_frames() }
    }

    fn stream_fps(&self) -> f32 {
        self.get_stream_fps()
    }
}

#[cfg(not(target_arch = "wasm32"))]
type DecoderSlot = Arc<Mutex<Option<fframes_media::FFmpegDecoder>>>;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct DecoderEntry {
    /// The preview job that used the decoder last, see `DecoderPool::finish_frame`.
    last_used: u64,
    /// The order of the last `DecoderPool::slot` call, for `MAX_DECODERS_PER_VIDEO`.
    last_access: u64,
    slot: DecoderSlot,
}

/// What a decoder of a file is used for. Each key has its own decoder, so two uses of
/// one file at different positions do not seek the same decoder back and forth.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderKey {
    /// `get_synced_video_frame` at a playback speed.
    Speed(PlaybackSpeed),
    /// `video_frame_at` with this key (one per `Clip`).
    Source(u64),
}

#[cfg(not(target_arch = "wasm32"))]
impl From<PlaybackSpeed> for DecoderKey {
    fn from(speed: PlaybackSpeed) -> Self {
        Self::Speed(speed)
    }
}

/// Each decoder keeps the file open and its frames in memory. A file used with more
/// keys closes the idle decoder that was used least recently.
#[cfg(not(target_arch = "wasm32"))]
const MAX_DECODERS_PER_VIDEO: usize = 4;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct DecoderPool {
    /// The decoders of each file, one per playback speed and one per clip: offsets of
    /// different speeds and clips point to different parts of the file.
    sources: HashMap<PathBuf, Vec<(DecoderKey, DecoderEntry)>>,
    generation: u64,
    accesses: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl DecoderPool {
    fn slot(&mut self, path: &Path, key: impl Into<DecoderKey>) -> DecoderSlot {
        let key = key.into();
        self.accesses += 1;
        let decoders = if let Some(decoders) = self.sources.get_mut(path) {
            decoders
        } else {
            self.sources.entry(path.to_owned()).or_default()
        };
        let index = if let Some(index) = decoders.iter().position(|(k, _)| *k == key) {
            index
        } else {
            if decoders.len() >= MAX_DECODERS_PER_VIDEO
                && let Some(idle) = decoders
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, entry))| Arc::strong_count(&entry.slot) == 1)
                    .min_by_key(|(_, (_, entry))| entry.last_access)
                    .map(|(index, _)| index)
            {
                decoders.remove(idle);
            }
            decoders.push((key, DecoderEntry::default()));
            decoders.len() - 1
        };
        let entry = &mut decoders[index].1;
        entry.last_used = self.generation;
        entry.last_access = self.accesses;
        Arc::clone(&entry.slot)
    }

    /// The decoders of `path` at every speed and of every clip. A bare file name finds an unambiguous source.
    fn slots_of(&self, path: &Path) -> Vec<DecoderSlot> {
        let decoders = self.sources.get(path).or_else(|| {
            let mut matches = self
                .sources
                .iter()
                .filter(|(source, _)| source.file_name() == Some(path.as_os_str()));
            let (_, decoders) = matches.next()?;
            matches.next().is_none().then_some(decoders)
        });
        decoders
            .map(|decoders| {
                decoders
                    .iter()
                    .map(|(_, entry)| Arc::clone(&entry.slot))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn finish_frame(&mut self, concurrent_jobs: usize) -> Vec<DecoderSlot> {
        // Count completed jobs, not timeline positions. Scrubs, loops and out-of-order
        // workers share the same grace period. Keep all layers of an active composition.
        self.generation += 1;
        let generation = self.generation;
        let grace = concurrent_jobs.max(1) as u64 + 1;
        let mut retired = Vec::new();
        self.sources.retain(|_, decoders| {
            retired.extend(
                decoders
                    .extract_if(.., |(_, entry)| {
                        generation - entry.last_used > grace && Arc::strong_count(&entry.slot) == 1
                    })
                    .map(|(_, entry)| entry.slot),
            );
            !decoders.is_empty()
        });
        retired
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct VideoDecodersWorker {
    buffer_size: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pool: Arc<Mutex<DecoderPool>>,
    #[cfg(not(target_arch = "wasm32"))]
    mode: fframes_media::DecodeMode,
}

impl std::fmt::Debug for VideoDecodersWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoDecodersWorker")
            .finish_non_exhaustive()
    }
}

impl Default for VideoDecodersWorker {
    fn default() -> Self {
        Self::new(1)
    }
}

impl VideoDecodersWorker {
    pub fn new(buffer_size: usize) -> Self {
        VideoDecodersWorker {
            buffer_size,
            #[cfg(not(target_arch = "wasm32"))]
            pool: Arc::default(),
            #[cfg(not(target_arch = "wasm32"))]
            mode: fframes_media::DecodeMode::Configured,
        }
    }

    /// The native studio keeps only recently used source decoders between frames.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn for_preview(software_decode: bool) -> Self {
        Self {
            mode: if software_decode {
                fframes_media::DecodeMode::Software
            } else {
                fframes_media::DecodeMode::Preview
            },
            ..Self::default()
        }
    }

    /// Retire idle decoder sessions outside the pool lock, including after a seek
    /// into a scene that contains no video files.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn finish_preview_frame(&self, concurrent_jobs: usize) {
        let retired = self
            .pool
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finish_frame(concurrent_jobs);
        drop(retired);
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn get_synced_frame<'media>(
        &self,
        media_ref: &VideoMedia,
        offset: i64,
        ctx: &FFramesContext<'_, 'media>,
        options: &SyncVideoFrameInput<'media>,
    ) -> crate::error::Result<Option<SourceFrame<'media>>> {
        let Some(metadata) = media_ref.metadata else {
            return Ok(None);
        };

        // The editor decodes on the grid of the video's frames.
        let speed = i64::from(PlaybackSpeed::new(options.speed).thousandths());
        let scale = i64::from(PlaybackSpeed::SCALE);
        let offset = (offset * speed + scale - 1) / scale;

        let last_frame_to_display_image = metadata.duration * ctx.time_base.fps as f32;
        if offset > last_frame_to_display_image as i64 && !options.looping {
            return Ok(None);
        }

        Ok(editor_frame(
            media_ref,
            metadata,
            offset,
            ctx,
            options.editor_fallback_image,
        ))
    }

    /// The frame at `seconds` of the file in the editor: the frame of the decode queue
    /// on the grid of the video's fps, or the fallback image.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn get_frame_at_source<'f>(
        &self,
        media_ref: &VideoMedia,
        seconds: f64,
        _key: u64,
        ctx: &FFramesContext<'_, '_>,
        editor_fallback_image: Option<&'f ImageData<'f>>,
    ) -> crate::error::Result<Option<SourceFrame<'f>>> {
        let Some(metadata) = media_ref.metadata else {
            return Ok(None);
        };
        if seconds >= f64::from(metadata.duration) {
            return Ok(None);
        }

        let offset = (seconds * ctx.time_base.fps as f64).round() as i64;
        Ok(editor_frame(
            media_ref,
            metadata,
            offset,
            ctx,
            editor_fallback_image,
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_buffered_frame_image(
        &self,
        pts: i64,
        filename: &str,
    ) -> Result<Option<Arc<usvgr::PreloadedImageData>>> {
        let slots = self.pool.lock()?.slots_of(Path::new(filename));
        for slot in slots {
            let image = slot
                .lock()?
                .as_ref()
                .and_then(|decoder| decoder.get_decoded_image_in_buf(pts));
            if image.is_some() {
                return Ok(image);
            }
        }
        Ok(None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn get_synced_frame<'media>(
        &self,
        VideoMedia { path, .. }: &VideoMedia,
        offset: i64,
        ctx: &FFramesContext<'_, 'media>,
        options: &SyncVideoFrameInput,
    ) -> Result<Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>>> {
        // A slot serializes the cursor of this source at this speed. Opening a file,
        // decoding a frame or tearing down a hardware session never holds the pool lock.
        let speed = PlaybackSpeed::new(options.speed);
        let slot = self.pool.lock()?.slot(path, speed);
        let mut guard = slot.lock()?;
        unsafe {
            let decoder = match guard.as_mut() {
                Some(decoder) => decoder,
                None => guard.insert(
                    fframes_media::FFmpegDecoder::new_with_mode(
                        path,
                        ctx.time_base.fps,
                        self.buffer_size,
                        self.mode,
                    )?
                    .with_playback_speed(speed),
                ),
            };
            let has_frame = if options.looping {
                decoder.decode_looping(offset)?
            } else {
                decoder.decode_up_to(offset)?
            };
            Ok(has_frame.then(|| decoder.retained_frame()).transpose()?)
        }
    }

    /// The frame at `seconds` of the file, decoded by the decoder of `key`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn get_frame_at_source<'f>(
        &self,
        VideoMedia { path, .. }: &VideoMedia,
        seconds: f64,
        key: u64,
        ctx: &FFramesContext<'_, '_>,
        _editor_fallback_image: Option<&'f ImageData<'f>>,
    ) -> Result<Option<SourceFrame<'f>>> {
        let slot = self.pool.lock()?.slot(path, DecoderKey::Source(key));
        let mut guard = slot.lock()?;
        unsafe {
            let decoder = match guard.as_mut() {
                Some(decoder) => decoder,
                None => guard.insert(fframes_media::FFmpegDecoder::new_with_mode(
                    path,
                    ctx.time_base.fps,
                    self.buffer_size,
                    self.mode,
                )?),
            };
            let has_frame = decoder.decode_at_seconds(seconds)?;
            Ok(has_frame.then(|| decoder.retained_frame()).transpose()?)
        }
    }
}

/// The frame at `offset` (frames of the video's fps) from the editor's decode queue, or
/// the fallback image while it is not decoded.
#[cfg(target_arch = "wasm32")]
fn editor_frame<'f>(
    media_ref: &VideoMedia,
    metadata: GeneralVideoFileMetadata,
    offset: i64,
    ctx: &FFramesContext<'_, '_>,
    editor_fallback_image: Option<&'f ImageData<'f>>,
) -> Option<SourceFrame<'f>> {
    use crate::media::{ImageMetadata, OwnedSharedString};

    // Call into JS to get a decoded video frame from the buffer manager.
    // The callback is synchronous — it returns a buffered frame or None.
    if let Some(filename) = media_ref.path.file_name().map(|f| f.to_string_lossy()) {
        let is_preview = matches!(ctx.mode, crate::FFramesMode::EditorTimelinePreview);
        let frame_data = VIDEO_FRAME_RESOLVER.with(|resolver| {
            resolver
                .get()
                .and_then(|resolve| resolve(&filename, offset, is_preview))
        });

        if let Some(data) = frame_data {
            // preview_url (data URL) is used for timeline canvas rendering
            // where SVG-as-image blocks blob URL resolution.
            // Falls back to the main url if no preview_url is provided.
            let preview_source = data.preview_url.as_deref().unwrap_or(&data.url);

            return Some(Arc::new(WasmEditorVideoFrameFallback {
                fallback_image: ImageData::new_from_web_source(
                    OwnedSharedString::new_owned(preview_source.to_owned()),
                    data.url,
                    format!("__video_frame_{}_{}", filename, offset),
                    ImageMetadata {
                        width: data.width,
                        height: data.height,
                    },
                ),
                video_metadata: metadata,
            }));
        }
    }

    // Fall back to static fallback image (no decoded frames available yet)
    Some(Arc::new(WasmEditorVideoFrameFallback {
        fallback_image: editor_fallback_image?.clone(),
        video_metadata: metadata,
    }))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod decoder_pool_tests {
    use super::*;

    #[test]
    fn clips_age_out_without_evicting_active_layers_or_in_flight_work() {
        let mut pool = DecoderPool::default();
        let first = Path::new("first/clip.mp4");
        let second = Path::new("second/clip.mp4");
        let normal = PlaybackSpeed::NORMAL;
        let held = pool.slot(first, normal);
        assert!(!Arc::ptr_eq(&held, &pool.slot(second, normal)));
        for _ in 0..24 {
            // Overlapping sources in a composite must all stay warm.
            for i in 0..12 {
                pool.slot(Path::new(&format!("layer-{i}.mp4")), normal);
            }
            drop(pool.finish_frame(5));
        }
        assert!(!pool.sources.contains_key(second));
        assert!(Arc::ptr_eq(&held, &pool.slot(first, normal)));
        assert_eq!(pool.sources.len(), 13);
        drop(held);
        for _ in 0..9 {
            drop(pool.finish_frame(5));
        }
        assert!(pool.sources.is_empty());
    }

    #[test]
    fn a_file_keeps_one_decoder_per_speed_and_closes_the_idle_one_used_least_recently() {
        let mut pool = DecoderPool::default();
        let clip = Path::new("clip.mp4");
        let normal = pool.slot(clip, PlaybackSpeed::new(1.0));
        // Speeds that round to the same thousandth share a decoder.
        assert!(Arc::ptr_eq(
            &normal,
            &pool.slot(clip, PlaybackSpeed::new(1.000_1))
        ));
        let held = pool.slot(clip, PlaybackSpeed::new(2.0));
        for speed in [0.5, 1.5] {
            pool.slot(clip, PlaybackSpeed::new(speed));
        }
        assert_eq!(pool.sources[clip].len(), MAX_DECODERS_PER_VIDEO);
        drop(normal);
        // A fifth speed closes the idle decoder used least recently, not the one in use.
        pool.slot(clip, PlaybackSpeed::new(3.0));
        let keys: Vec<_> = pool.sources[clip].iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [2.0, 0.5, 1.5, 3.0].map(|speed| DecoderKey::Speed(PlaybackSpeed::new(speed)))
        );
        assert_eq!(pool.slots_of(Path::new("clip.mp4")).len(), 4);
        drop(held);
    }

    #[test]
    fn each_clip_of_a_file_has_its_own_decoder() {
        let mut pool = DecoderPool::default();
        let clip = Path::new("clip.mp4");
        let synced = pool.slot(clip, PlaybackSpeed::NORMAL);
        let face = pool.slot(clip, DecoderKey::Source(1));
        let pip = pool.slot(clip, DecoderKey::Source(2));
        assert!(!Arc::ptr_eq(&synced, &face));
        assert!(!Arc::ptr_eq(&face, &pip));
        assert!(Arc::ptr_eq(&face, &pool.slot(clip, DecoderKey::Source(1))));
        // the renderer finds the converted pixels of every decoder of the file
        assert_eq!(pool.slots_of(clip).len(), 3);
    }
}
