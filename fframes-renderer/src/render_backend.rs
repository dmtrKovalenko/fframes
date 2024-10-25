use crate::{
    encoder::EncoderOptions, fframes_logger::FFramesLogger, renderer_error::FFramesRendererResult,
};
use fframes::{usvgr, AudioTimelineSamples, ResolvedRenderingTimeline, Video};
use std::sync::Arc;
use usvgr::fontdb;

// TODO: feature flag this
// pub use crate::gpu::GpuRenderingBackend;

#[allow(clippy::too_many_arguments)]
pub trait FFramesRenderBackend {
    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        &self,
        frame: fframes::Frame,
        out: &str,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()>;

    fn render<'a, TVideo: Video + Sync + Sized>(
        &self,
        output: &'a str,
        video: &'a TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &usvgr::Options,
        encoder_options: &EncoderOptions<'a>,
        font_db: &fontdb::Database,
        timeline: &ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: fframes::FFramesContext<'a, '_>,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized;
}
