use crate::{FFramesContext, media::GeneralVideoFileMetadata, media::ImageData};
use fframes_media::VideoMedia;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
#[cfg(not(target_arch = "wasm32"))]
use fframes_media::ImageMetadata;
pub use fframes_media::{FrameConvertOptions, ResizeVideoFrame};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(target_arch = "wasm32")]
use std::cell::Cell;

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

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncVideoFrameInput<'media> {
    /// The seconds timestamp in seconds to start playing the video from
    /// If not provided starts playing from the beginning of the video or scene.
    pub start_from: f32,
    /// If `true` the video playback will be looped and will start from the beginning on the end.
    /// Also if `true` the `get_synced_video_frame` API should never return None, only in case of
    /// decoding errors, so in some cases it might be okay to panic if the frame is not available.
    pub looping: bool,
    /// The wasm preview editor is not supporting decoding video frames in runtime
    /// so this allows to provide a fallback image for the debugging
    pub editor_fallback_image: Option<&'media ImageData<'media>>,
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
    last_used: u64,
    slot: DecoderSlot,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct DecoderPool {
    sources: HashMap<PathBuf, DecoderEntry>,
    generation: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl DecoderPool {
    fn slot(&mut self, path: &Path) -> DecoderSlot {
        let entry = if let Some(entry) = self.sources.get_mut(path) {
            entry
        } else {
            self.sources.entry(path.to_owned()).or_default()
        };
        entry.last_used = self.generation;
        Arc::clone(&entry.slot)
    }

    fn finish_frame(&mut self, concurrent_jobs: usize) -> Vec<DecoderSlot> {
        // Count completed jobs, not timeline positions. Scrubs, loops and out-of-order
        // workers share the same grace period. Keep all layers of an active composition.
        self.generation += 1;
        self.sources
            .extract_if(|_, entry| {
                self.generation - entry.last_used > concurrent_jobs.max(1) as u64 + 1
                    && Arc::strong_count(&entry.slot) == 1
            })
            .map(|(_, entry)| entry.slot)
            .collect()
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
    ) -> crate::error::Result<Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>>> {
        use crate::media::{ImageMetadata, OwnedSharedString};

        let Some(metadata) = media_ref.metadata else {
            return Ok(None);
        };

        let last_frame_to_display_image = metadata.duration * ctx.time_base.fps as f32;
        if offset > last_frame_to_display_image as i64 && !options.looping {
            return Ok(None);
        }

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

                return Ok(Some(Arc::new(WasmEditorVideoFrameFallback {
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
                })));
            }
        }

        // Fall back to static fallback image (no decoded frames available yet)
        let Some(fallback_image) = options.editor_fallback_image else {
            return Ok(None);
        };

        Ok(Some(Arc::new(WasmEditorVideoFrameFallback {
            fallback_image: fallback_image.clone(),
            video_metadata: metadata,
        })))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_buffered_frame_image(
        &self,
        pts: i64,
        filename: &str,
    ) -> Result<Option<Arc<usvgr::PreloadedImageData>>> {
        let decoder = {
            let pool = self.pool.lock()?;
            let path = Path::new(filename);
            pool.sources
                .get(path)
                .or_else(|| {
                    // Older callers use a bare filename. Only resolve an unambiguous source.
                    let mut matches = pool
                        .sources
                        .iter()
                        .filter(|(source, _)| source.file_name() == Some(path.as_os_str()));
                    let (_, entry) = matches.next()?;
                    matches.next().is_none().then_some(entry)
                })
                .map(|entry| Arc::clone(&entry.slot))
        };
        let Some(slot) = decoder else { return Ok(None) };
        Ok(slot
            .lock()?
            .as_ref()
            .and_then(|decoder| decoder.get_decoded_image_in_buf(pts)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn get_synced_frame<'media>(
        &self,
        VideoMedia { path, .. }: &VideoMedia,
        mut offset: i64,
        ctx: &FFramesContext<'_, 'media>,
        options: &SyncVideoFrameInput,
    ) -> Result<Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>>> {
        // A slot serializes this source's cursor. Opening a file, decoding a frame
        // or tearing down a hardware session never holds the shared pool lock.
        let slot = self.pool.lock()?.slot(path);
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
            if options.looping {
                offset = decoder.adjust_offset_for_looping(offset)?;
            }

            let has_frame = decoder.decode_up_to(offset)?;
            if has_frame {
                Ok(Some(decoder.retained_frame()?))
            } else if options.looping {
                decoder.seek_to_offset(0)?;
                let has_first_frame = decoder.decode_up_to(0)?;

                decoder.current_loop += 1;
                Ok(has_first_frame
                    .then(|| decoder.retained_frame())
                    .transpose()?)
            } else {
                Ok(None)
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod decoder_pool_tests {
    use super::*;

    #[test]
    fn clips_age_out_without_evicting_active_layers_or_in_flight_work() {
        let mut pool = DecoderPool::default();
        let first = Path::new("first/clip.mp4");
        let second = Path::new("second/clip.mp4");
        let held = pool.slot(first);
        assert!(!Arc::ptr_eq(&held, &pool.slot(second)));
        for _ in 0..24 {
            // Overlapping sources in a composite must all stay warm.
            for i in 0..12 {
                pool.slot(Path::new(&format!("layer-{i}.mp4")));
            }
            drop(pool.finish_frame(5));
        }
        assert!(!pool.sources.contains_key(second));
        assert!(Arc::ptr_eq(&held, &pool.slot(first)));
        assert_eq!(pool.sources.len(), 13);
        drop(held);
        for _ in 0..9 {
            drop(pool.finish_frame(5));
        }
        assert!(pool.sources.is_empty());
    }
}
