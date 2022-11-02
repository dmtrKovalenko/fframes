use encoder::EncoderOptions;
use fframes::video::Video;
use fframes::{fframes_context, AudioData};
use fframes_logger::FFramesLoggerVariant;
use render_backend::FFramesRenderBackend;

mod concatenator;
mod encoder;
mod ffmpeg_helper;
pub mod fframes_logger;
pub use fframes_logger::*;
use renderer_error::FFramesResult;
mod gpu;
mod media_processor;
pub mod render_backend;
mod renderer_error;
mod stream;

#[derive(Debug, Clone, Default)]
pub struct RenderOptions<'a, TBackend: FFramesRenderBackend> {
    pub media_dir: &'a str,
    pub logger: FFramesLoggerVariant,
    pub encoder_options: EncoderOptions<'a>,
    pub render_backend: TBackend,
    pub default_font: &'a str,
    /// Preferred codec ot use. If not allowed to use will use default codec for the container which may not be the most efficient.
    /// Because libav by default ignores non-system codecs like hevc or x264.
    pub preferred_codec: &'a str,
}

pub fn render<'a, TVideo: Video + Sync + Sized, TBackend: FFramesRenderBackend>(
    video: TVideo,
    output: &'a str,
    options: RenderOptions<'a, TBackend>,
) -> FFramesResult<()> {
    let fps = TVideo::FPS;
    let logger = fframes_logger::make_logger(options.logger);

    let (media_provider, image_data) =
        media_processor::load_media_from_folder(&logger, options.media_dir).unwrap();

    let mut opt = usvgr::Options {
        image_data,
        font_family: options.default_font.to_string(),
        ..Default::default()
    };

    opt.fontdb.load_system_fonts();

    media_provider.fonts.values().for_each(|font_path| {
        opt.fontdb
            .load_font_file(font_path)
            .unwrap_or_else(|_| println!("Can not load a font"));
    });

    let (duration_in_frames, scenes) =
        fframes::video::resolve_duration_and_scenes_sync(&video, |name| {
            media_provider
                .audio
                .get(name)
                .map(|main_audio| match main_audio {
                    AudioData::Preloaded(data) => {
                        data.samples.len() / data.sample_rate as usize * fps
                    }
                    _ => 0,
                })
                .ok_or_else(|| {
                    fframes::error::FFramesCoreError::CanNotProcessAudioDuration(name.to_owned())
                })
        })?;

    let ctx = fframes_context::FFramesContext {
        sample_rate: 44100,
        mode: fframes::FFramesMode::Renderer,
        fps,
        media_provider,
        duration_in_frames,
        scenes: scenes.as_ref(),
    };

    logger.init_frames_rendering(duration_in_frames);

    options.render_backend.render(
        output,
        video,
        logger,
        &opt.to_ref(),
        duration_in_frames,
        options.encoder_options,
        ctx,
    )?;

    Ok(())
}

/// Renders a single frame into the output image file.
/// Prints all the rendering warns and errors for the frame along with the svg file itself.
/// Compiles only for debug target.
pub fn debug_frame<'a, TVideo: Video + Sync + Sized, TBackend: FFramesRenderBackend>(
    frame_index: usize,
    video: TVideo,
    output_png: &'a str,
    options: RenderOptions<'a, TBackend>,
) -> FFramesResult<()> {
    let logger = fframes_logger::make_logger(options.logger);
    let (media_provider, image_data) =
        media_processor::load_media_from_folder(&logger, options.media_dir).unwrap();

    let mut opt = usvgr::Options {
        image_data,
        font_family: options.default_font.to_string(),
        ..Default::default()
    };

    let (_, scenes) = fframes::video::resolve_duration_and_scenes_sync(&video, |name| {
        media_provider
            .audio
            .get(name)
            .map(|main_audio| match main_audio {
                AudioData::Preloaded(data) => {
                    data.samples.len() / data.sample_rate as usize * TVideo::FPS
                }
                _ => 0,
            })
            .ok_or_else(|| {
                fframes::error::FFramesCoreError::CanNotProcessAudioDuration(name.to_owned())
            })
    })?;

    // let relative_frame_index = scenes
    //     .and_then(|scenes| scenes.find_relative_index(frame_index))
    //     .unwrap_or(frame_index);
    opt.fontdb.load_system_fonts();

    media_provider.fonts.values().for_each(|font_path| {
        opt.fontdb
            .load_font_file(font_path)
            .unwrap_or_else(|_| println!("Can not load a font"));
    });

    let ctx = fframes_context::FFramesContext {
        sample_rate: 44100,
        mode: fframes::FFramesMode::Renderer,
        fps: TVideo::FPS,
        media_provider,
        duration_in_frames: 1,
        scenes: scenes.as_ref(),
    };

    options.render_backend.debug_frame(
        fframes::Frame {
            index: frame_index,
            global_index: frame_index,
            fps: TVideo::FPS,
        },
        output_png,
        video,
        &opt.to_ref(),
        ctx,
    )
}
