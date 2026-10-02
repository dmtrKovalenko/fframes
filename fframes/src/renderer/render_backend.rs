use super::frame_export::{
    EncoderFrameRenderer, EncoderInput, RgbaEncoderFrameRenderer, VideoEncoderInfo,
};
use super::renderer_error::{FFramesRendererError, RenderEncodingResult};
use super::{fframes_logger::FFramesLogger, renderer_error::FFramesRendererResult};
use crate::{AudioTimelineSamples, RenderOptions, ResolvedRenderingTimeline, Video, usvgr};
use std::{path::Path, sync::Arc};
use usvgr::fontdb;

#[allow(clippy::too_many_arguments)]
pub trait FFramesRenderBackend {
    /// Renders single frames (`fframes::Previewer`, the CLI's `frame`, `strip`, `onion` and
    /// `snapshot`) the way this backend renders the video, so previews match the output.
    /// `None` uses the built-in CPU renderer.
    fn frame_renderer(&self) -> Option<impl super::FrameRenderer + '_> {
        None::<std::convert::Infallible>
    }

    /// Picks what the video encoders of a render are opened with, before any of them
    /// exists. A backend that can produce the encoder's frames natively answers with the
    /// format it delivers: another software format it converts to on the GPU, or
    /// hardware frames ([`EncoderInput::hardware_frames`]) the encoder reads without the
    /// pixels ever being copied into memory.
    ///
    /// The default keeps the pixel format requested in `EncoderOptions`.
    fn negotiate_encoder_input(
        &self,
        encoder: &VideoEncoderInfo<'_>,
    ) -> RenderEncodingResult<EncoderInput> {
        EncoderInput::requested(encoder)
    }

    /// Rasterizes frames into what the encoder takes (`input` is the outcome of
    /// [`Self::negotiate_encoder_input`]), one renderer per rendering thread.
    ///
    /// The default renders RGBA with [`Self::frame_renderer`] and converts it on the CPU.
    fn encoder_frame_renderer(
        &self,
        input: &EncoderInput,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<impl EncoderFrameRenderer + '_> {
        let renderer = self.frame_renderer().ok_or_else(|| {
            FFramesRendererError::Custom(
                "the rendering backend has no frame renderer to produce encoder frames with"
                    .to_owned(),
            )
        })?;

        RgbaEncoderFrameRenderer::new(renderer, input, width, height)
            .map_err(|err| FFramesRendererError::from_chunk(0, err))
    }

    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        frame: crate::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: crate::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>>;

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        encoder_options: &'a RenderOptions<'a, 'media>,
        font_db: &'a fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a crate::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized;
}
