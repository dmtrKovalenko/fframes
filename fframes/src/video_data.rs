use crate::{media::GeneralVideoFileMetadata, media::ImageData, FFramesContext};
use fframes_media_loaders::VideoMedia;
pub use fframes_media_loaders::{FrameConvertOptions, ResizeVideoFrame};
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use fframes_media_loaders::ImageMetadata;

#[cfg(not(target_arch = "wasm32"))]
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub trait FFramesSyncedVideoFrame {
    /// Gets the original image from the frame
    /// This will perform color space conversion but will preserve the original frame
    /// size. If you need to render the video frame in the smaller size, use `into_resized_image`.
    ///
    /// This method take self to reuse frame data allocations, but you should not
    /// call this method more than once per the render function
    #[allow(clippy::wrong_self_convention)]
    fn into_image(&self) -> ImageData;
    /// Performs color space conversion and resizes the image to the specified size.
    /// Recommended to use this method if you render image in the smaller size than the original
    /// video e.g. as a pattern of some other shape or as a filler of standalone image.
    ///
    /// This method take self to reuse frame data allocations, but you should not
    /// call this method more than once per the render function
    #[allow(clippy::wrong_self_convention)]
    fn into_resized_image(&self, options: &FrameConvertOptions) -> Option<ImageData>;
    /// Get the native height of the frame
    fn height(&self) -> u32;
    /// Get the native size of the frame
    fn width(&self) -> u32;
    /// Get the frame index of the origin video
    fn timestamp_seconds(&self) -> f64;
    /// Get the FPS of the origin video
    fn stream_fps(&self) -> f64;
    /// Get the duration of the origin video in frames
    fn stream_duration_in_seconds(&self) -> f64;
}

/// This is used by the editor as a fallback for the static image
#[derive(Debug, Clone)]
pub struct WasmEditorVideoFrameFallback {
    fallback_image: ImageData,
    video_metadata: GeneralVideoFileMetadata,
}

impl FFramesSyncedVideoFrame for WasmEditorVideoFrameFallback {
    fn into_image(&self) -> ImageData {
        self.fallback_image.clone()
    }

    fn into_resized_image(&self, _options: &FrameConvertOptions) -> Option<ImageData> {
        Some(self.fallback_image.clone())
    }

    fn height(&self) -> u32 {
        self.video_metadata.height
    }

    fn width(&self) -> u32 {
        self.video_metadata.width
    }

    fn timestamp_seconds(&self) -> f64 {
        unimplemented!(
            "Timestamp seconds are not supported in the editor. Please mock this data for editing."
        )
    }

    fn stream_fps(&self) -> f64 {
        self.video_metadata.fps
    }

    fn stream_duration_in_seconds(&self) -> f64 {
        self.video_metadata.duration
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl FFramesSyncedVideoFrame for fframes_media_loaders::FFmpegFrame {
    fn into_image(&self) -> ImageData {
        let image_data = unsafe { self.get_image() };

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

    fn into_resized_image(&self, options: &FrameConvertOptions) -> Option<ImageData> {
        let image_data = unsafe {
            self.reinit_sws_context(options)
                .map_err(|e| {
                    crate::log!("Error while decoding video frame: {:?}", e);
                })
                .ok()?;

            self.get_image()
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

    fn height(&self) -> u32 {
        self.get_width()
    }

    fn width(&self) -> u32 {
        self.get_height()
    }

    fn timestamp_seconds(&self) -> f64 {
        unsafe { self.timestamp_seconds() }
    }

    fn stream_duration_in_seconds(&self) -> f64 {
        unsafe { self.get_stream_duration_in_frames() }
    }

    fn stream_fps(&self) -> f64 {
        self.get_stream_fps()
    }
}

#[derive(Clone)]
/// Cheap to clone set of video decoers per render worker
pub struct WorkerLocalVideoDecoders {
    #[cfg(not(target_arch = "wasm32"))]
    map: Rc<RefCell<HashMap<String, fframes_media_loaders::FFmpegDecoder>>>,
}

impl std::fmt::Debug for WorkerLocalVideoDecoders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerLocalDecoders").finish()
    }
}

impl Default for WorkerLocalVideoDecoders {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerLocalVideoDecoders {
    pub fn new() -> Self {
        WorkerLocalVideoDecoders {
            #[cfg(not(target_arch = "wasm32"))]
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn get_synced_frame(
        &self,
        media_ref: &VideoMedia,
        _offset: usize,
        ctx: &FFramesContext,
    ) -> crate::error::Result<Option<Arc<impl FFramesSyncedVideoFrame>>> {
        let filename = media_ref.path.file_name().unwrap().to_string_lossy();
        let video_filename = filename.as_ref();

        if let Some(image) = ctx
            .get_image(format!("{video_filename}_fallback.png"))
            .or_else(|| ctx.get_image(format!("{video_filename}.fallback.jpg")))
        {
            Ok(Some(Arc::new(WasmEditorVideoFrameFallback {
                fallback_image: image.clone(),
                video_metadata: media_ref
                    .metadata
                    .expect("Failed precodition: Missing editor video file metadata"),
            })))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn get_synced_frame(
        &self,
        VideoMedia { path, .. }: &VideoMedia,
        offset: usize,
        ctx: &FFramesContext,
    ) -> crate::error::Result<Option<Arc<impl FFramesSyncedVideoFrame>>> {
        let has_decoder = {
            (*self.map)
                .borrow()
                .contains_key(path.to_string_lossy().as_ref())
        };

        if !has_decoder {
            let decoder = unsafe {
                let mut decoder =
                    fframes_media_loaders::FFmpegDecoder::new(path, ctx.time_base.fps)?;
                if offset > 0 {
                    decoder.seek_to_offset(offset as i64)?;
                }

                decoder
            };

            (*self.map)
                .borrow_mut()
                .insert(path.to_string_lossy().to_string(), decoder);
        }

        unsafe {
            let mut decoders = (*self.map).borrow_mut();
            let decoder = decoders.get_mut(path.to_string_lossy().as_ref()).unwrap();

            let has_frame = decoder.decode_up_to(offset as i64)?;

            if has_frame {
                Ok(Some(decoder.get_raw_frame()))
            } else {
                Ok(None)
            }
        }
    }
}
