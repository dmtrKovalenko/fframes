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

#[derive(Debug, Clone, Default)]
pub struct SyncVideoFrameInput {
    /// The frame of the fframes' video to start from.
    /// If not provided starts from the beginning of the fframes' video.
    ///
    /// It is possible to leverage the scene's offset to start video from the scene start:
    ///
    /// ```no_run
    /// use fframes::{Frame, SyncVideoFrameInput};
    ///
    /// let scene_info = ctx.get_scene_info();
    ///
    /// fnrame.get_synced_video_frame(ctx, "video.mp4", SyncVideoFrameInput {
    ///    start_from: scene_info.map(|info| info.start_frame),
    ///    ..Default::default()
    /// });
    ///
    /// ```
    pub start_from: usize,
    /// If `true` the video playback will be looped and will start from the beginning on the end.
    /// Also if `true` the `get_synced_video_frame` API should never return None, only in case of
    /// decoding errors, so in some cases it might be okay to panic if the frame is not available.
    pub looping: bool,
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
        _offset: i64,
        ctx: &FFramesContext,
        _options: &SyncVideoFrameInput,
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
        offset: i64,
        ctx: &FFramesContext,
        options: &SyncVideoFrameInput,
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

                decoder.seek_to_offset(0)?;
                if options.looping {
                    let frame = decoder.get_raw_frame();
                    let output_video_length_in_seconds =
                        (ctx.duration_in_frames / ctx.time_base.fps) as f64;
                    // looping with frame decoding is expensive, so skip if the video is longer than the output video

                    if frame.stream_duration_in_seconds() < output_video_length_in_seconds {
                        // use a guess to avoid decoding through large videos
                        let end_of_video_index_guess =
                            (frame.stream_duration_in_seconds() * ctx.time_base.fps as f64) as i64;

                        decoder.end_of_video_index = end_of_video_index_guess;
                        loop {
                            decoder.seek_to_offset(decoder.end_of_video_index)?;
                            let has_frame = decoder.decode_up_to(decoder.end_of_video_index)?;
                            if !has_frame {
                                break;
                            }
                            decoder.end_of_video_index += 1;
                        }
                        if end_of_video_index_guess == decoder.end_of_video_index {
                            // now scroll back to make sure we have the correct end_of_video_index
                            let mut rollback = decoder.end_of_video_index - 1;
                            loop {
                                decoder.seek_to_offset(rollback)?;
                                let has_frame = decoder.decode_up_to(rollback)?;
                                if has_frame {
                                    decoder.end_of_video_index = rollback + 1;
                                    break;
                                }
                                rollback -= 1;
                            }
                        }

                        decoder.seek_to_offset(0)?;
                    } else {
                        decoder.end_of_video_index = ctx.duration_in_frames as i64;
                    }
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

            let mut target_offset = offset;
            if options.looping {
                target_offset = offset % decoder.end_of_video_index;
                if target_offset == 0 {
                    decoder.seek_to_offset(0)?;
                }
            }

            let has_frame = decoder.decode_up_to(target_offset)?;
            match has_frame {
                true => Ok(Some(decoder.get_raw_frame())),
                false => Ok(None),
            }
        }
    }
}
