use super::{fframes_logger::FFramesLogger, renderer_error::FFramesRendererResult};
use crate::{AudioTimelineSamples, RenderOptions, ResolvedRenderingTimeline, Video, usvgr};
use std::{path::Path, sync::Arc};
use usvgr::fontdb;

#[allow(clippy::too_many_arguments)]
pub trait FFramesRenderBackend {
    /// Renders single frames (`fframes::Previewer`, the CLI's `frame`, `strip`, `onion` and
    /// `snapshot`) the way this backend renders the video, so previews match the output.
    /// `None` uses the built-in CPU renderer.
    fn frame_renderer(&self) -> Option<Box<dyn super::FrameRenderer + '_>> {
        None
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
