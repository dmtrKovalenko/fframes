use fframes_renderer::rayon::prelude::*;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::skia_pipeline::Pipeline;
pub use crate::skia_pipeline::{SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};
use crate::{resource_provider::SkiaFFramesProvider, skia_pipeline};
use fframes::{
    usvgr::{self, WriteOptions},
    AudioTimelineSamples, ResolvedRenderingTimeline, Video,
};
use fframes_renderer::{
    concatenator, FFramesRenderBackend, FFramesRendererError, FFramesRendererResult,
};
use skia_safe::{gpu, surfaces::raster_n32_premul, svg::Dom, Surface};
use uuid::Uuid;

pub(crate) struct SkiaContext {
    pub(crate) gpu_context: Option<skia_safe::gpu::DirectContext>,
    pub(crate) surface: Surface,
}

#[derive(Clone)]
pub struct SkiaFFramesRenderer {
    pub(crate) frame_datavec_size: usize,
    pub(crate) pipeline_config: SkiaPipelineConfig,
    pub(crate) skia: Arc<Mutex<SkiaContext>>,
}

unsafe impl Send for SkiaFFramesRenderer {}
unsafe impl Sync for SkiaFFramesRenderer {}

impl SkiaFFramesRenderer {
    /// Creates a new skia render with an optional gpu context and an abstracted surface.
    /// If you are using metal or vulkan consider using prebuild `new_metal` and `new_vulkan`
    /// constructors for the reasonable defaults.
    pub fn new(
        pipeline_config: SkiaPipelineConfig,
        mut surface: Surface,
        gpu_context: Option<gpu::DirectContext>,
    ) -> Self {
        let image_info = surface.image_info();
        let frame_byte_size = image_info.compute_byte_size(image_info.min_row_bytes());

        Self {
            pipeline_config,
            frame_datavec_size: frame_byte_size,
            #[allow(clippy::arc_with_non_send_sync)]
            skia: Arc::new(Mutex::new(SkiaContext {
                gpu_context,
                surface,
            })),
        }
    }

    /// Creates new CPU based skia renderer. It is not recommended to use it for the final
    /// video rendering, if you are rendering a video on a target without GPU consider
    /// using a built-in CPU renderer.
    ///
    /// Intended for compatibility layer with GPU renderer and/or testing.
    pub fn new_cpu(
        pipeline_config: SkiaPipelineConfig,
        width: usize,
        height: usize,
    ) -> FFramesRendererResult<Self> {
        let mut surface = raster_n32_premul((width as i32, height as i32)).ok_or_else(|| {
            fframes_renderer::FFramesRendererError::Custom(
                "Failed to create skia surface".to_string(),
            )
        })?;

        let image_info = surface.image_info();
        let frame_byte_size = image_info.compute_byte_size(image_info.min_row_bytes());

        Ok(Self {
            frame_datavec_size: frame_byte_size,
            pipeline_config,
            #[allow(clippy::arc_with_non_send_sync)]
            skia: Arc::new(Mutex::new(SkiaContext {
                gpu_context: None,
                surface,
            })),
        })
    }
}

impl FFramesRenderBackend for SkiaFFramesRenderer {
    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let mut skia = self.skia.lock()?;
        let SkiaContext {
            ref mut gpu_context,
            ref mut surface,
        } = *skia;

        let mut pixels = vec![0; self.frame_datavec_size];
        let image_info = surface.image_info();
        let pixmap = skia_safe::Pixmap::new(&image_info, &mut pixels, image_info.min_row_bytes())
            .ok_or_else(|| {
            fframes_renderer::FFramesRendererError::Custom("Failed to create pixmap".to_string())
        })?;

        let mut converter_cache = usvgr::Cache::default();
        let rtree = video.render_frame(frame, &ctx).into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let provider = SkiaFFramesProvider::new(&self.pipeline_config, &ctx);
        let raw_svg = rtree.to_string(&WriteOptions::default());

        let dom = Dom::from_str(&raw_svg, provider).unwrap();
        dom.render(surface.canvas());
        if let Some(gpu_context) = gpu_context {
            gpu_context.flush_and_submit();
        }

        let image = { surface.image_snapshot() };
        let result = image.read_pixels_to_pixmap_with_context(
            gpu_context,
            &pixmap,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        );

        if !result {
            return Err(fframes_renderer::FFramesRendererError::Custom(
                "Failed to read pixels from Skia image".to_string(),
            ));
        }

        Ok(pixels)
    }

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: std::sync::Arc<dyn fframes_renderer::FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        encoder_options: &'a fframes_renderer::EncoderOptions<'a>,
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
                let threads_available = fframes_renderer::get_thread_count();
                threads_available / 3
            }
            SkiaPipelineConcurrencyPolicy::Concurrency(pipelines) => pipelines,
            SkiaPipelineConcurrencyPolicy::OnePipeline => 1,
        };

        if concurrent_pipelines < 2 {
            let provider = SkiaFFramesProvider::new(&self.pipeline_config, ctx);
            skia_pipeline::start(Pipeline {
                output,
                include_audio: true,
                frame_range: &(0..ctx.duration_in_frames),
                skia: &self,
                provider: &provider,
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
                .ok_or(fframes_renderer::FFramesRendererError::InvalidOutput)?;

            let files = chunks
                .par_iter()
                .enumerate()
                .map(|(file, chunk)| {
                    let provider = SkiaFFramesProvider::new(&self.pipeline_config, ctx);
                    let file = directory.join(format!(
                        "{file}.{extension}",
                        file = file,
                        extension = extension.to_string_lossy().as_ref()
                    ));

                    skia_pipeline::start(Pipeline {
                        include_audio: false,
                        frame_range: chunk,
                        output: &file,
                        skia: &self,
                        provider: &provider,
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
