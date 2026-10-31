use crate::{FFramesContext, media::GeneralVideoFileMetadata, media::ImageData};
use fframes_media::VideoMedia;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
#[cfg(not(target_arch = "wasm32"))]
use fframes_media::ImageMetadata;
pub use fframes_media::{FrameConvertOptions, PlaybackSpeed, ResizeVideoFrame};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

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
    /// Gets the original image from the frame
    /// This will perform color space conversion and return the frame at `width()` by
    /// `height()`. If you need to render the video frame in the smaller size, use `into_resized_image`.
    ///
    /// This method take self to reuse frame data allocations, but you should not
    /// call this method more than once per the render function
    #[allow(clippy::wrong_self_convention)]
    fn into_image(&self) -> ImageData<'media>;
    /// Performs color space conversion and resizes the image to the specified size.
    /// Recommended to use this method if you render image in the smaller size than the original
    /// video e.g. as a pattern of some other shape or as a filler of standalone image.
    ///
    /// This method take self to reuse frame data allocations, but you should not
    /// call this method more than once per the render function
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
    /// The seconds timestamp in seconds to start playing the video from
    /// If not provided starts playing from the beginning of the video or scene.
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
impl<'media> FFramesSyncedVideoFrame<'media> for crate::media::FFmpegFrameBuf {
    fn into_image(&self) -> ImageData<'media> {
        let image_data = unsafe { self.convert_last_decoded_frame_into_svg_image(None) }
            // we unwrap here because we guarantee that the sws scaler would work correctly
            // unless the wrong dimensions are provided
            .expect("Failed to convert frame to image");

        let filename = image_data.id.clone();
        ImageData::new_from_raw_data(
            image_data,
            filename,
            ImageMetadata {
                width: self.get_width(),
                height: self.get_height(),
            },
        )
    }

    fn into_resized_image(&self, options: &FrameConvertOptions) -> Option<ImageData<'media>> {
        let image_data = unsafe { self.convert_last_decoded_frame_into_svg_image(Some(options)) }
            .map_err(|e| {
                eprintln!("Error converting frame to image: {e:?}");
            })
            .ok()?;

        let filename = image_data.id.clone();
        Some(ImageData::new_from_raw_data(
            image_data,
            filename,
            ImageMetadata {
                width: self.get_width(),
                height: self.get_height(),
            },
        ))
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

#[derive(Clone)]
#[allow(dead_code)]
pub struct VideoDecodersWorker {
    buffer_size: usize,
    #[cfg(not(target_arch = "wasm32"))]
    map: Arc<std::sync::RwLock<HashMap<String, Vec<SpeedDecoder>>>>,
}

/// A decoder of a file at one playback speed.
#[cfg(not(target_arch = "wasm32"))]
type SpeedDecoder = (PlaybackSpeed, fframes_media::FFmpegDecoder);

/// Each decoder keeps the file open and its frames in memory. A file played at more
/// speeds closes the decoder that was used least recently.
#[cfg(not(target_arch = "wasm32"))]
const MAX_DECODERS_PER_VIDEO: usize = 4;

impl std::fmt::Debug for VideoDecodersWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerLocalDecoders").finish()
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
            map: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
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

        // The editor decodes on the grid of the video's frames.
        let speed = i64::from(PlaybackSpeed::new(options.speed).thousandths());
        let scale = i64::from(PlaybackSpeed::SCALE);
        let offset = (offset * speed + scale - 1) / scale;

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
        let frame = self.map.read().unwrap().get(filename).and_then(|decoders| {
            decoders
                .iter()
                .find_map(|(_, decoder)| decoder.get_decoded_image_in_buf(pts))
        });

        Ok(frame)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn get_synced_frame<'media>(
        &self,
        VideoMedia { path, .. }: &VideoMedia,
        offset: i64,
        ctx: &FFramesContext<'_, 'media>,
        options: &SyncVideoFrameInput,
    ) -> Result<Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>>> {
        use crate::error::FFramesError;
        use fframes_media::FFramesMediaError;

        let resource_name = path
            .file_name()
            .ok_or_else(|| FFramesError::MediaError(FFramesMediaError::MediaDirectoryProvided))?
            .to_string_lossy();

        let speed = PlaybackSpeed::new(options.speed);

        // It is important to lock it for the whole duration of he function to
        // avoid any parallel encoder creation.
        let mut map_guard = self.map.write()?;
        // Every speed gets its own decoder, offsets of different speeds point to
        // different parts of the file.
        let decoders = map_guard.entry(resource_name.into_owned()).or_default();
        let decoder = decoder_for_speed(decoders, speed, || unsafe {
            fframes_media::FFmpegDecoder::new(path, ctx.time_base.fps, self.buffer_size)
                .map(|decoder| decoder.with_playback_speed(speed))
        })?;
        unsafe {
            let has_frame = if options.looping {
                decoder.decode_looping(offset)?
            } else {
                decoder.decode_up_to(offset)?
            };
            Ok(has_frame.then(|| decoder.get_raw_frame()))
        }
    }
}

/// The decoder of `speed`, made with `create` if there is none. The most recently used
/// decoder is first, and at most `MAX_DECODERS_PER_VIDEO` are kept.
#[cfg(not(target_arch = "wasm32"))]
fn decoder_for_speed<D, E>(
    decoders: &mut Vec<(PlaybackSpeed, D)>,
    speed: PlaybackSpeed,
    create: impl FnOnce() -> std::result::Result<D, E>,
) -> std::result::Result<&mut D, E> {
    if let Some(index) = decoders.iter().position(|(s, _)| *s == speed) {
        decoders[..=index].rotate_right(1);
    } else {
        decoders.insert(0, (speed, create()?));
        decoders.truncate(MAX_DECODERS_PER_VIDEO);
    }
    Ok(&mut decoders[0].1)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod decoder_cache_tests {
    use super::*;

    fn use_speed(decoders: &mut Vec<(PlaybackSpeed, f32)>, speed: f32, created: &mut usize) {
        decoder_for_speed(decoders, PlaybackSpeed::new(speed), || {
            *created += 1;
            Ok::<_, ()>(speed)
        })
        .unwrap();
    }

    #[test]
    fn speeds_that_round_to_the_same_value_share_a_decoder() {
        let (mut decoders, mut created) = (Vec::new(), 0);
        for i in 0..100 {
            use_speed(&mut decoders, 1.0 + i as f32 * 0.000_001, &mut created);
        }
        assert_eq!((decoders.len(), created), (1, 1));
    }

    #[test]
    fn least_recently_used_decoders_are_closed() {
        let (mut decoders, mut created) = (Vec::new(), 0);
        for i in 0..100 {
            use_speed(&mut decoders, 1.0 + i as f32 * 0.1, &mut created);
        }
        assert_eq!((decoders.len(), created), (MAX_DECODERS_PER_VIDEO, 100));

        // A used decoder moves to the front and is kept.
        use_speed(&mut decoders, 1.0 + 97.0 * 0.1, &mut created);
        use_speed(&mut decoders, 0.25, &mut created);
        use_speed(&mut decoders, 0.5, &mut created);
        use_speed(&mut decoders, 1.0 + 97.0 * 0.1, &mut created);
        assert_eq!(created, 102);
        let speeds: Vec<_> = decoders.iter().map(|(s, _)| s.thousandths()).collect();
        assert_eq!(speeds, [10_700, 500, 250, 10_900]);
    }
}

// These implementations are already thread-safe due to RwLock
unsafe impl Send for VideoDecodersWorker {}
unsafe impl Sync for VideoDecodersWorker {}
