use crate::{
    encoder::EncoderOptions, fframes_logger::FFramesLogger, renderer_error::FFramesRendererResult,
};
use fframes::{usvgr, AudioTimelineSamples, ResolvedRenderingTimeline, Video};
use std::sync::Arc;

pub use crate::gpu::GpuRenderingBackend;

#[allow(clippy::too_many_arguments)]
pub trait FFramesRenderBackend {
    #[cfg(debug_assertions)]
    fn debug_frame<TVideo: Video + Sync + Sized>(
        &self,
        frame: fframes::Frame,
        out: &str,
        video: &TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr_text_layout::fontdb::Database,
        ctx: fframes::FFramesContext,
    ) -> FFramesRendererResult<()>;

    fn render<'a, TVideo: Video + Sync + Sized>(
        &self,
        output: &'a str,
        video: &'a TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &usvgr::Options,
        encoder_options: &EncoderOptions<'a>,
        font_db: &usvgr_text_layout::fontdb::Database,
        timeline: &ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: fframes::FFramesContext,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized;
}
