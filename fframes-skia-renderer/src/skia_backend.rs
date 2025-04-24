use fframes::rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;

use crate::backends::SkiaBackend;
use crate::skia_pipeline::Pipeline;
pub use crate::skia_pipeline::{SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};
use crate::{resource_provider::SkiaFFramesProvider, skia_pipeline};
use fframes::{
    AudioTimelineSamples, ResolvedRenderingTimeline, Video,
    usvgr::{self, WriteOptions},
};
use fframes::{
    FFramesRenderBackend, FFramesRendererError, FFramesRendererResult, VideoDecodersWorker,
    concatenator,
};
use skia_safe::svg::Dom;
use uuid::Uuid;

#[derive(Clone)]
pub struct SkiaFFramesRenderer<'a, T: SkiaBackend + Sync + Send> {
    pub(crate) pipeline_config: SkiaPipelineConfig,
    pub(crate) backend: &'a T,
}

impl<'a, TSkiaBackend: SkiaBackend> SkiaFFramesRenderer<'a, TSkiaBackend> {
    /// Creates a new Skia render with based on the supported skia rendering backend.
    /// Currently only Vulkan and Metal are supported (also CPU for testing purposes).
    ///
    /// Check `new_metal` and `new_vulkan` methods for more details.
    pub fn new(
        pipeline_config: SkiaPipelineConfig,
        skia_backend_context: &'a TSkiaBackend,
    ) -> Self {
        Self {
            pipeline_config,
            backend: skia_backend_context,
        }
    }
}

impl<TBackend: SkiaBackend> FFramesRenderBackend for SkiaFFramesRenderer<'_, TBackend> {
    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let (mut surface, mut gpu_context) = self.backend.create_skia_surface()?;
        let image_info = surface.image_info();
        let frame_datavec_size = image_info.compute_byte_size(image_info.min_row_bytes());

        let mut pixels = vec![0; frame_datavec_size];
        let pixmap = skia_safe::Pixmap::new(&image_info, &mut pixels, image_info.min_row_bytes())
            .ok_or_else(|| {
            fframes::FFramesRendererError::Custom("Failed to create pixmap".to_string())
        })?;

        let mut converter_cache = usvgr::Cache::default();
        let rtree = video.render_frame(frame, &ctx).into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let video_decoders_worker = VideoDecodersWorker::new(1);
        let provider = SkiaFFramesProvider::new(video_decoders_worker, &ctx);
        let raw_svg = rtree.to_string(&WriteOptions::default());

        let dom = Dom::from_str(&raw_svg, provider).unwrap();
        dom.render(surface.canvas());

        if let Some(gpu_context) = gpu_context.as_mut() {
            gpu_context.flush_and_submit();
        }

        let image = surface.image_snapshot();
        let result = image.read_pixels_to_pixmap_with_context(
            gpu_context.as_mut(),
            &pixmap,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        );

        if !result {
            return Err(fframes::FFramesRendererError::Custom(
                "Failed to read pixels from Skia image".to_string(),
            ));
        }

        Ok(pixels)
    }

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: std::sync::Arc<dyn fframes::FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        encoder_options: &'a fframes::EncoderOptions<'a>,
        font_db: &'a usvgr::fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized,
    {
        let output = output.as_ref();
        let concurrent_pipelines = match self.pipeline_config.concurrency_policy {
            SkiaPipelineConcurrencyPolicy::MaxPerformance => {
                let threads_available = fframes::get_thread_count();
                threads_available / 3
            }
            SkiaPipelineConcurrencyPolicy::Concurrency(pipelines) => pipelines,
            SkiaPipelineConcurrencyPolicy::OnePipeline => 1,
        };

        if concurrent_pipelines < 2 {
            skia_pipeline::start(Pipeline {
                pipeline_config: self.pipeline_config,
                output,
                include_audio: true,
                frame_range: &(0..ctx.duration_in_frames),
                skia: self.backend,
                video,
                logger: Arc::clone(&logger),
                usvg_options,
                encoder_options,
                font_db,
                timeline,
                ctx,
            })?;

            logger.success(output, None);
        } else {
            let chunks =
                encoder_options.split_video_chunks(ctx.duration_in_frames, concurrent_pipelines);

            let session_id = Uuid::new_v4();
            let tmp_path = std::env::temp_dir().join(format!("fframes-skia-{session_id}"));
            let directory = encoder_options.tmp_files_directory.unwrap_or(&tmp_path);
            if !directory.exists() {
                std::fs::create_dir(directory)?;
            }

            let extension = output
                .extension()
                .ok_or(fframes::FFramesRendererError::InvalidOutput)?;

            let files = chunks
                .par_iter()
                .enumerate()
                .map(|(file, chunk)| {
                    let file = directory.join(format!(
                        "{file}.{extension}",
                        file = file,
                        extension = extension.to_string_lossy().as_ref()
                    ));

                    skia_pipeline::start(Pipeline {
                        pipeline_config: self.pipeline_config,
                        include_audio: false,
                        frame_range: chunk,
                        output: &file,
                        skia: self.backend,
                        video,
                        logger: Arc::clone(&logger),
                        usvg_options,
                        encoder_options,
                        font_db,
                        timeline,
                        ctx,
                    })?;

                    Ok(file)
                })
                .collect::<FFramesRendererResult<Vec<_>>>()?;

            unsafe {
                concatenator::concat_video_files_with_audio(
                    files.as_slice(),
                    output,
                    concurrent_pipelines as i32,
                    timeline.audio_map.as_ref(),
                    encoder_options,
                    ctx,
                )
                .map_err(FFramesRendererError::ConcatChunkError)?;
            }

            logger.success(output, Some(directory));
        }

        Ok(())
    }
}
