use crate::renderer_error::FFramesRendererResult;
use crate::renderer_font_source::RendererFontSource;
pub use encoder::{AVPixelFormat, AVSampleFormat, EncoderOptions};
use fframes::MediaProvider;
use fframes::Video;
use fframes::{AudioData, FFramesContext, ScenesWithAudio, TimeBase};
use std::collections::HashMap;
use usvgr::fontdb;

pub mod fframes_logger;

pub mod cpu;
// pub mod gpu;

mod concatenator;
mod encoder;
mod encoder_frame;
mod ffmpeg_helper;
mod media_directory;
mod render_backend;
mod renderer_error;
mod renderer_font_source;
mod stream;

pub use fframes_logger::*;
pub use media_directory::*;
pub use render_backend::*;

#[derive(Debug, Clone)]
/// All the final render-specific options applies to the final video rendering pipeline
/// including media resolution, logging, rendering backend, and encoding.
pub struct RenderOptions<'a, TBackend: FFramesRenderBackend> {
    pub width: usize,
    pub height: usize,
    pub fps: usize,
    pub media: Option<&'a (dyn MediaProvider<'a>)>,
    pub logger: FFramesLoggerVariant,
    pub encoder_options: EncoderOptions<'a>,
    pub render_backend: TBackend,
    /// If `true` locates and loads system font on MacOS, Windows and Linux OSes.
    /// It is anyway recommended to provide all the font as either statically and dynamically
    /// linked media.
    pub load_system_fonts: bool,
    /// The default "base" font family that will be used for the text elements without specified
    /// font family.
    ///
    /// @default "Times New Roman"
    pub default_font: &'a str,
}

impl<'a, TBackend: FFramesRenderBackend + Default> Default for RenderOptions<'a, TBackend> {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            media: None,
            logger: FFramesLoggerVariant::default(),
            encoder_options: EncoderOptions::default(),
            render_backend: TBackend::default(),
            load_system_fonts: false,
            default_font: "Times New Roman",
        }
    }
}
pub fn render<'a, 'media: 'a, TBackend: FFramesRenderBackend, TVideo: Video + Sync + Sized>(
    video: &'a TVideo,
    output: &'a str,
    options: &'a RenderOptions<'media, TBackend>,
) -> FFramesRendererResult<()> {
    let logger = fframes_logger::make_logger(options.logger.clone());
    let time_base = TimeBase {
        fps: options.fps,
        sample_rate: options.encoder_options.sample_rate,
    };

    let scenes = video.define_scenes();
    let timeline = fframes::resolve_timeline(
        &video.duration(),
        &ScenesWithAudio::new(&scenes),
        &time_base,
        &video.audio(),
        |name| {
            options
                .media
                .ok_or_else(|| {
                    fframes::error::FFramesError::RequiredAudioNotFound(name.to_owned())
                })?
                .resolve_audio(name)
                .and_then(|main_audio| match main_audio {
                    AudioData::Preloaded(data) => {
                        Some(data.samples.len() * options.fps / data.sample_rate as usize)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    fframes::error::FFramesError::CanNotProcessAudioDuration(name.to_owned())
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

    let ctx = FFramesContext {
        width: options.width,
        height: options.height,
        fps: options.fps,
        time_base: TimeBase {
            sample_rate: 44100,
            fps: options.fps,
        },
        mode: fframes::FFramesMode::Renderer,
        media_source: options.media,
        duration_in_frames: timeline.duration_in_frames,
        scenes: timeline.scenes.as_ref(),
        font_source: Some(&font_source),
    };

    let font_db = font_source.as_db_ref();
    options.render_backend.render(
        output,
        video,
        logger,
        &fframes::usvgr::Options {
            image_data: Some(&image_source),
            font_family: options.default_font.to_string(),
            ..Default::default()
        },
        &options.encoder_options,
        font_db,
        &timeline,
        ctx,
    )?;

    Ok(())
}

/// Renders a single frame into the output image file.
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
///    img_buffer.save(output_path)?;
/// ```
pub fn render_frame<
    'a,
    'media: 'a,
    TBackend: FFramesRenderBackend,
    TVideo: Video + Sync + Sized,
>(
    frame_index: usize,
    video: &'a TVideo,
    options: &RenderOptions<'media, TBackend>,
) -> FFramesRendererResult<Vec<u8>> {
    let time_base = TimeBase {
        fps: options.fps,
        sample_rate: options.encoder_options.sample_rate,
    };

    let scenes = video.define_scenes();
    let timeline: fframes::ResolvedRenderingTimeline<fframes::AudioTimelineSamples> =
        fframes::resolve_timeline(
            &video.duration(),
            &ScenesWithAudio::new(&scenes),
            &time_base,
            &video.audio(),
            |name| {
                options
                    .media
                    .ok_or_else(|| {
                        fframes::error::FFramesError::RequiredAudioNotFound(name.to_owned())
                    })?
                    .resolve_audio(name)
                    .and_then(|main_audio| match main_audio {
                        AudioData::Preloaded(data) => {
                            Some(data.samples.len() * options.fps / data.sample_rate as usize)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| {
                        fframes::error::FFramesError::CanNotProcessAudioDuration(name.to_owned())
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

    let ctx = FFramesContext {
        time_base: TimeBase {
            sample_rate: 44100,
            fps: options.fps,
        },
        width: options.width,
        height: options.height,
        fps: options.fps,
        mode: fframes::FFramesMode::Renderer,
        media_source: options.media,
        duration_in_frames: timeline.duration_in_frames,
        scenes: timeline.scenes.as_ref(),
        font_source: Some(&font_source),
    };

    let font_db = font_source.as_db_ref();
    options.render_backend.render_frame(
        fframes::Frame::new(frame_index, frame_index, options.fps),
        video,
        &fframes::usvgr::Options {
            image_data: Some(&image_source),
            font_family: options.default_font.to_string(),
            ..Default::default()
        },
        font_db,
        ctx,
    )
}
