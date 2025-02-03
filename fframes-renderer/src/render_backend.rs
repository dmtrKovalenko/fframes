use crate::{
    encoder::EncoderOptions, fframes_logger::FFramesLogger, renderer_error::FFramesRendererResult,
};
use fframes::{usvgr, AudioTimelineSamples, ResolvedRenderingTimeline, Video};
use std::{path::Path, sync::Arc};
use usvgr::fontdb;

// TODO: feature flag this
// pub use crate::gpu::GpuRenderingBackend;

#[allow(clippy::too_many_arguments)]
pub trait FFramesRenderBackend {
    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>>;

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        encoder_options: &'a EncoderOptions<'a>,
        font_db: &'a fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized;
}
