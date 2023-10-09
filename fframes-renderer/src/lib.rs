use crate::renderer_font_source::RendererFontSource;
pub use encoder::{AVPixelFormat, AVSampleFormat, EncoderOptions};
use fframes::MediaProvider;
use fframes::Video;
use fframes::{AudioData, FFramesContext, ScenesWithAudio, TimeBase};
use fframes_logger::FFramesLoggerVariant;
use render_backend::FFramesRenderBackend;
use renderer_error::FFramesRendererResult;
use std::collections::HashMap;

pub mod fframes_logger;

pub mod cpu;
pub mod gpu;

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

#[derive(Debug, Clone, Default)]
/// All the final render-specific options applies to the video includes media providing, loggin,
/// and encoding.
pub struct RenderOptions<'a, TBackend: FFramesRenderBackend> {
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

pub fn render<'a, TBackend: FFramesRenderBackend, TVideo: Video + Sync + Sized>(
    video: &'a TVideo,
    output: &'a str,
    options: RenderOptions<'a, TBackend>,
) -> FFramesRendererResult<()> {
    let logger = fframes_logger::make_logger(options.logger.clone());
    let time_base = TimeBase {
        fps: TVideo::FPS,
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
                        Some(data.samples.len() * TVideo::FPS / data.sample_rate as usize)
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
        fontdb: usvgr_text_layout::fontdb::Database::new(),
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
            fps: TVideo::FPS,
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

///// Renders a single frame into the output image file.
///// Prints all the rendering warns and errors for the frame along with the svg file itself.
/////
///// Compiles only for debug target.
// #[cfg(debug_assertions)]
// pub fn debug_frame<'a, 'media: 'a, TBackend: FFramesRenderBackend, TVideo: Video + Sync + Sized>(
//     frame_index: usize,
//     video: TVideo,
//     output_png: &'a str,
//     options: RenderOptions<'a, 'media, TBackend>,
// ) -> FFramesRendererResult<()> {
//     // let (font_db, media_provider, timeline, image_data, _) =
//     //     prepare_rendering_context(&options, &video)?;
//     //
//     // let font_source = RendererFontSource { fontdb: &font_db };
//     // let ctx = FFramesContext {
//     //     time_base: TimeBase {
//     //         sample_rate: 44100,
//     //         fps: TVideo::FPS,
//     //     },
//     //     mode: fframes::FFramesMode::Renderer,
//     //     media_provider: &media_provider,
//     //     duration_in_frames: 1,
//     //     scenes: None,
//     //     font_source: Some(&font_source),
//     // };
//     //
//     // options.render_backend.debug_frame(
//     //     fframes::Frame {
//     //         index: frame_index,
//     //         global_index: frame_index,
//     //         fps: TVideo::FPS,
//     //         breaks_lru_cache: None,
//     //     },
//     //     output_png,
//     //     &video,
//     //     &usvgr::Options {
//     //         image_data: Some(&image_data),
//     //         font_family: options.default_font.to_string(),
//     //         ..Default::default()
//     //     },
//     //     &font_db,
//     //     ctx,
//     // )
//     todo!()
// }
