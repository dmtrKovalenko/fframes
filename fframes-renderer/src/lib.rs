use std::collections::HashMap;
use std::sync::Arc;

use encoder::EncoderOptions;
use fframes::media_provider::MediaProvider;
use fframes::video::Video;
use fframes::{fframes_context, usvgr, AudioData, ResolvedScenesTimeline};
use fframes_logger::FFramesLoggerVariant;
use futures::executor::block_on;
use render_backend::FFramesRenderBackend;

mod concatenator;
mod encoder;
mod ffmpeg_helper;
pub mod fframes_logger;
mod renderer_font_source;
pub use fframes_logger::*;
use renderer_error::FFramesResult;
use usvgr::PreloadedImageData;

use crate::renderer_font_source::RendererFontSource;

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

type RenderPreparation = (
    usize,
    MediaProvider,
    usvgr_text_layout::fontdb::Database,
    Option<ResolvedScenesTimeline>,
    HashMap<String, Arc<PreloadedImageData>>,
    Arc<dyn FFramesLogger>,
);

pub async fn prepare_rendering_context<
    'a,
    TVideo: Video + Sync + Sized,
    TBackend: FFramesRenderBackend,
>(
    options: &'a RenderOptions<'a, TBackend>,
    video: &'a TVideo,
) -> FFramesResult<RenderPreparation> {
    let logger = fframes_logger::make_logger(options.logger.clone());
    let (media_provider, font_db, image_data) =
        media_processor::load_media_from_folder(&logger, options.media_dir, TVideo::FPS).unwrap();

    let media_provider = Arc::new(media_provider);
    let (final_duration, scenes) =
        fframes::video::resolve_duration_and_scenes_async(video, |name| {
            let provider = Arc::clone(&media_provider);

            Box::pin(async move {
                provider
                    .audio
                    .get(name.as_str())
                    .map(|main_audio| match main_audio {
                        AudioData::Preloaded(data) => {
                            data.samples.len() / data.sample_rate as usize * TVideo::FPS
                        }
                        _ => 0,
                    })
                    .ok_or_else(|| {
                        fframes::error::FFramesCoreError::CanNotProcessAudioDuration(
                            name.to_owned(),
                        )
                    })
            })
        })
        .await?;

    Ok((
        final_duration,
        Arc::try_unwrap(media_provider).unwrap(),
        font_db,
        scenes,
        image_data,
        logger,
    ))
}

pub fn render<'a, TVideo: Video + Sync + Sized, TBackend: FFramesRenderBackend>(
    video: TVideo,
    output: &'a str,
    options: RenderOptions<'a, TBackend>,
) -> FFramesResult<()> {
    let (duration_in_frames, media_provider, font_db, scenes, image_data, logger) =
        block_on(prepare_rendering_context(&options, &video))?;

    logger.init_frames_rendering(duration_in_frames);

    let font_source = RendererFontSource { fontdb: &font_db };
    let ctx = fframes_context::FFramesContext {
        sample_rate: 44100,
        mode: fframes::FFramesMode::Renderer,
        fps: TVideo::FPS,
        media_provider: &media_provider,
        duration_in_frames,
        scenes: scenes.as_ref(),
        font_source: Some(&font_source),
    };

    options.render_backend.render(
        output,
        video,
        logger,
        &fframes::usvgr::Options {
            image_data: Some(&image_data),
            font_family: options.default_font.to_string(),
            ..Default::default()
        },
        options.encoder_options,
        &font_db,
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
    let (_, media_provider, font_db, scenes, image_data, _) =
        block_on(prepare_rendering_context(&options, &video))?;

    let font_source = RendererFontSource { fontdb: &font_db };
    let ctx = fframes_context::FFramesContext {
        sample_rate: 44100,
        mode: fframes::FFramesMode::Renderer,
        fps: TVideo::FPS,
        media_provider: &media_provider,
        duration_in_frames: 1,
        scenes: scenes.as_ref(),
        font_source: Some(&font_source),
    };

    options.render_backend.debug_frame(
        fframes::Frame {
            index: frame_index,
            global_index: frame_index,
            fps: TVideo::FPS,
            breaks_lru_cache: None,
        },
        output_png,
        video,
        &usvgr::Options {
            image_data: Some(&image_data),
            font_family: options.default_font.to_string(),
            ..Default::default()
        },
        &font_db,
        ctx,
    )
}
