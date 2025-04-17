#![allow(clippy::missing_safety_doc)]
use crate::MediaProvider;
use crate::Video;
use crate::VideoDecodersWorker;
use crate::VideoSize;
use crate::usvgr::fontdb;
use crate::{AudioData, FFramesContext, ScenesWithAudio, TimeBase};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "cpu_renderer")]
pub mod cpu;
#[doc(hidden)]
pub mod pix_fmt;

pub mod concatenator;
mod encoder;
mod encoder_frame;
mod ffmpeg_helper;
pub mod fframes_logger;
mod media_directory;
mod render_backend;
mod renderer_error;
mod renderer_font_source;
mod stream;

pub use encoder::*;
pub use fframes_logger::*;
pub use media_directory::*;
pub use rayon;
pub use render_backend::*;
pub use renderer_error::*;
use renderer_font_source::RendererFontSource;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
/// All the final render-specific options applies to the final video rendering pipeline
/// including media resolution, logging, rendering backend, and encoding.
pub struct RenderOptions<'a> {
    pub media: Option<&'a (dyn MediaProvider<'a>)>,
    pub logger: FFramesLoggerVariant,
    pub encoder_options: EncoderOptions<'a>,
    pub override_fps: Option<usize>,
    pub scale_resolution: f64,
    /// If `true` locates and loads system font on MacOS, Windows and Linux OSes.
    /// It is anyway recommended to provide all the font as either statically and dynamically
    /// linked media.
    ///
    /// Keep in mind if you set it to `true` it might not work on "their" machine.
    pub load_system_fonts: bool,
    /// The default "base" font family that will be used for the text elements without specified
    /// font family.
    ///
    /// @default "Arial"
    pub default_font: &'a str,
}

impl Default for RenderOptions<'_> {
    fn default() -> Self {
        Self {
            media: None,
            scale_resolution: 1.0,
            logger: FFramesLoggerVariant::Compact,
            encoder_options: Default::default(),
            override_fps: None,
            load_system_fonts: false,
            default_font: "Arial",
        }
    }
}

fn create_context<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
    _video: &'a TVideo,
    options: &'a RenderOptions<'media>,
    timeline: &'a crate::ResolvedRenderingTimeline<'a, crate::AudioTimelineSamples>,
    font_source: &'a RendererFontSource,
) -> FFramesContext<'a, 'media> {
    FFramesContext {
        time_base: TimeBase {
            sample_rate: 44100,
            fps: options.override_fps.unwrap_or(TVideo::FPS),
        },
        mode: crate::FFramesMode::Renderer,
        media_source: options.media,
        duration_in_frames: timeline.duration_in_frames,
        scenes: timeline.scenes.as_ref(),
        font_source: Some(font_source),
        current_video_size: VideoSize::new_scaled(
            TVideo::WIDTH,
            TVideo::HEIGHT,
            options.scale_resolution,
        ),
    }
}

/// Renders fframes video to the output destination path. Uses the provided video and media
/// implementations as long as the reneder backend implementation.
///
/// The `RenderOptions::encoder_options` field might be used to configure all the final
/// video file properties like bitrate, quality, video and audio codecs options, etc.
pub fn render<
    'a,
    'media: 'a,
    TBackend: FFramesRenderBackend,
    TVideo: Video + Sync + Sized + Send,
>(
    output: impl AsRef<Path>,
    video: &'a TVideo,
    render_backend: TBackend,
    options: &'a RenderOptions<'media>,
) -> FFramesRendererResult<()> {
    let logger = fframes_logger::make_logger(options.logger.clone());
    let time_base = TimeBase {
        fps: TVideo::FPS,
        sample_rate: options.encoder_options.sample_rate,
    };

    let output = PathBuf::from(output.as_ref());
    let scenes = video.define_scenes();
    let timeline = crate::resolve_timeline(
        &video.duration(),
        &ScenesWithAudio::new(&scenes),
        &time_base,
        &video.audio(),
        |name| {
            options
                .media
                .ok_or_else(|| crate::error::FFramesError::RequiredAudioNotFound(name.to_owned()))?
                .resolve_audio(name)
                .and_then(|main_audio| match main_audio {
                    AudioData::Preloaded(data) => {
                        Some(data.samples.len() * TVideo::FPS / data.sample_rate as usize)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    crate::error::FFramesError::CanNotProcessAudioDuration(name.to_owned())
                })
        },
    )?;

    logger.init_frames_rendering(timeline.duration_in_frames)?;

    let mut image_source = HashMap::new();
    let mut font_source = RendererFontSource {
        fontdb: fontdb::Database::new(),
    };

    if options.load_system_fonts {
        font_source.fontdb.load_system_fonts();
    }

    if let Some(media) = options.media {
        media.populate_font_source(&mut font_source);
        media.populate_image_source(&mut image_source);
    }

    let ctx = create_context(video, options, &timeline, &font_source);
    let font_db = font_source.as_db_ref();

    render_backend.render(
        &output,
        video,
        logger,
        &crate::usvgr::Options {
            image_data: Some(&image_source),
            font_family: options.default_font.to_string(),
            ..Default::default()
        },
        &options.encoder_options,
        font_db,
        &timeline,
        &ctx,
    )?;

    Ok(())
}

/// Renders a single frame into the output image buffer.
/// Prints all the rendering warns and errors for the frame along with the svg file itself.
/// Returns a byte representation specific to the render backend used.
/// For CpuRenderBackend it is a RGBA image of the video size.
///
/// Convert the RGBA output to image using `image` crate:
///
/// ```ignore
///    let frame_buffer = fframes_renderer::render_frame(...)?;
///    let img_buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(your_video::WIDTH as u32, your_video::HEIGHT as u32, frame_buffer)?;
///
///    img_buffer.save("output_path.png")?;
/// ```
pub fn render_frame<
    'a,
    'media: 'a,
    TBackend: FFramesRenderBackend,
    TVideo: Video + Sync + Sized + Send,
>(
    frame_index: usize,
    video: &'a TVideo,
    render_backend: TBackend,
    options: &RenderOptions<'media>,
) -> FFramesRendererResult<Vec<u8>> {
    let time_base = TimeBase {
        fps: TVideo::FPS,
        sample_rate: options.encoder_options.sample_rate,
    };

    let scenes = video.define_scenes();
    let timeline = crate::resolve_timeline(
        &video.duration(),
        &ScenesWithAudio::new(&scenes),
        &time_base,
        &video.audio(),
        |name| {
            options
                .media
                .ok_or_else(|| crate::error::FFramesError::RequiredAudioNotFound(name.to_owned()))?
                .resolve_audio(name)
                .and_then(|main_audio| match main_audio {
                    AudioData::Preloaded(data) => {
                        Some(data.samples.len() * TVideo::FPS / data.sample_rate as usize)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    crate::error::FFramesError::CanNotProcessAudioDuration(name.to_owned())
                })
        },
    )?;

    let mut image_source = HashMap::new();
    let mut font_source = RendererFontSource {
        fontdb: fontdb::Database::new(),
    };

    if options.load_system_fonts {
        font_source.fontdb.load_system_fonts();
    }

    if let Some(media) = options.media {
        media.populate_font_source(&mut font_source);
        media.populate_image_source(&mut image_source);
    }

    let ctx = create_context(video, options, &timeline, &font_source);
    let font_db = font_source.as_db_ref();
    let decoders = VideoDecodersWorker::new(1);

    render_backend.render_frame(
        crate::Frame::__internal_make_for_renderer(
            frame_index,
            frame_index,
            TVideo::FPS,
            None,
            decoders,
        ),
        video,
        &crate::usvgr::Options {
            image_data: Some(&image_source),
            font_family: options.default_font.to_string(),
            ..Default::default()
        },
        font_db,
        ctx,
    )
}

/// Convenient function that can be used by different backend implementation to
/// save the reused about of threads for the rendering process.
/// Allowing to override the default thread count using the `FFRAMES_NUN_THREADS` environment variable.
pub fn get_thread_count() -> usize {
    if let Some(threads) = std::env::var("FFRAMES_NUM_THREADS")
        .ok()
        .and_then(|threads| threads.parse().ok())
    {
        return threads;
    }

    rayon::current_num_threads()
}
