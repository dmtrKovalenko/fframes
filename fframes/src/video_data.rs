use crate::{FFramesContext, media::GeneralVideoFileMetadata, media::ImageData};
use fframes_media_loaders::VideoMedia;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::error::Result;
#[cfg(not(target_arch = "wasm32"))]
use fframes_media_loaders::ImageMetadata;
pub use fframes_media_loaders::{FrameConvertOptions, ResizeVideoFrame};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

pub trait FFramesSyncedVideoFrame<'media> {
    /// Gets the original image from the frame
    /// This will perform color space conversion but will preserve the original frame
    /// size. If you need to render the video frame in the smaller size, use `into_resized_image`.
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
    /// Get the native height of the frame
    fn height(&self) -> u32;
    /// Get the native size of the frame
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
impl<'media> FFramesSyncedVideoFrame<'media> for fframes_media_loaders::FFmpegFrameBuf {
    fn into_image(&self) -> ImageData<'media> {
        let image_data = unsafe { self.scale_and_return_latest_frame() };

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
        let image_data = unsafe {
            self.reinit_sws_context(options)
                .map_err(|e| {
                    crate::log!("Error while decoding video frame: {:?}", e);
                })
                .ok()?;

            self.scale_and_return_latest_frame()
        };

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

    /// Returns the original stream video width
    fn width(&self) -> u32 {
        self.get_stream_width()
    }

    /// Returns the original stream video height
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
    map: Arc<std::sync::RwLock<HashMap<String, fframes_media_loaders::FFmpegDecoder>>>,
}

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
        ctx: &FFramesContext,
        options: &SyncVideoFrameInput<'media>,
    ) -> crate::error::Result<Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>>> {
        let Some(fallback_image) = options.editor_fallback_image else {
            return Ok(None);
        };

        let Some(metadata) = media_ref.metadata else {
            return Ok(None);
        };

        let last_frame_to_display_image = metadata.duration * ctx.time_base.fps as f32;
        if offset <= last_frame_to_display_image as i64 || options.looping {
            Ok(Some(Arc::new(WasmEditorVideoFrameFallback {
                // this is only editor code + the image data is a smartpointer so it is okay to clone
                fallback_image: fallback_image.clone(),
                video_metadata: media_ref
                    .metadata
                    .expect("Failed precodition: Missing editor video file metadata"),
            })))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_buffered_frame_image(
        &self,
        pts: i64,
        filename: &str,
    ) -> Result<Option<Arc<usvgr::PreloadedImageData>>> {
        let frame = self
            .map
            .read()
            .unwrap()
            .get(filename)
            .and_then(|decoder| decoder.get_decoded_image_in_buf(pts));

        Ok(frame)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn get_synced_frame<'media>(
        &self,
        VideoMedia { path, .. }: &VideoMedia,
        mut offset: i64,
        ctx: &FFramesContext<'_, 'media>,
        options: &SyncVideoFrameInput,
    ) -> Result<Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>>> {
        use crate::error::FFramesError;
        use fframes_media_loaders::FFramesMediaError;

        let resource_name = path
            .file_name()
            .ok_or_else(|| FFramesError::MediaError(FFramesMediaError::MediaDirectoryProvided))?
            .to_string_lossy()
            .to_string();

        // It is important to lock it for the whole duration of he function to
        // avoid any parallel encoder creation.
        let mut map_guard = self.map.write()?;
        let needs_new_decoder = !map_guard.contains_key(&resource_name);

        // Create new decoder if needed
        if needs_new_decoder {
            let decoder = unsafe {
                let mut decoder = fframes_media_loaders::FFmpegDecoder::new(
                    path,
                    ctx.time_base.fps,
                    self.buffer_size,
                )?;

                if offset > 0 {
                    decoder.seek_to_offset(offset)?;
                }

                decoder
            };

            map_guard.insert(resource_name.clone(), decoder);
        }

        unsafe {
            // Use write lock for modifying decoder state
            let decoder = map_guard.get_mut(&resource_name).unwrap();

            if options.looping {
                offset = decoder.adjust_offset_for_looping(offset)?;
            }

            let has_frame = decoder.decode_up_to(offset)?;
            match has_frame {
                true => Ok(Some(decoder.get_raw_frame())),
                false if options.looping => {
                    decoder.seek_to_offset(0)?;
                    let has_first_frame = decoder.decode_up_to(0)?;

                    decoder.current_loop += 1;
                    Ok(has_first_frame.then_some(decoder.get_raw_frame()))
                }
                false => Ok(None),
            }
        }
    }
}

// These implementations are already thread-safe due to RwLock
unsafe impl Send for VideoDecodersWorker {}
unsafe impl Sync for VideoDecodersWorker {}
