use super::{
    EncoderFrameRenderer, EncoderInput, FrameRenderer, FrameScheduler, RgbaFrameConverter,
    SegmentWriter, VideoFrame, get_thread_count, render_backend::FFramesRenderBackend,
    renderer_error::RenderEncodingError,
};
use crate::{
    AbortSignal, AudioTimelineSamples, Frame, RenderOptions, ResolvedRenderingTimeline, TextCache,
    Video, VideoDecodersWorker, usvgr,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use svgr::{PixmapPool, SvgrCache, tiny_skia::Color};
use usvgr::fontdb;
use uuid::Uuid;

use super::{
    concatenator,
    fframes_logger::FFramesLogger,
    renderer_error::{FFramesRendererError, FFramesRendererResult},
};

/// Default rendering backend for fframes that uses pure rust SVG rendering engine and extremely
/// portable. It does not use GPU acceleration at all but still very competitive because of
/// layer caching and SIMD optimizations.
#[derive(Debug, Clone, Copy)]
pub struct CpuRenderingBackend {
    /// The number of **individual svg elements or groups** to cache. Pure CPU rendering is very slow
    /// for mostly any filter, shadows, or gradients so it is important to cache unchanged elements.
    /// At the same time do not set this to the unreasonably large values as it will consume a lot
    /// of memory and will decrease cache efficiently.
    ///
    /// The optimal size = general number of static (not animating) elements in your video.
    ///
    /// @default `20`
    pub cache_capacity: usize,
    /// The number of threads to use for rendering. By default it will use the number of logical cores on your machine.
    /// There is no reason to set this to a value greater than the number of logical cores because each thread will render its own video which after will be concatenated.
    ///
    /// @default `rayon::current_num_threads()`
    pub concurrency: usize,
    /// The number of `frame.text_break_lines` results to be cached.
    /// Text rendering and wrapping is very expensive especially on CPU as it involves a lot of text shaping and layout along with font resolution.
    pub text_cache_capacity: usize,
}

impl Default for CpuRenderingBackend {
    fn default() -> Self {
        Self {
            cache_capacity: 20,
            text_cache_capacity: 10,
            concurrency: get_thread_count(),
        }
    }
}

/// The CPU rasterizer as an [`EncoderFrameRenderer`]: tiny-skia draws premultiplied RGBA
/// that is converted into the encoder's pixel format with [`RgbaFrameConverter`].
pub struct CpuEncoderFrameRenderer {
    pixmap: svgr::tiny_skia::Pixmap,
    cache: SvgrCache,
    pixmap_pool: PixmapPool,
    context: svgr::Context,
    converter: RgbaFrameConverter,
}

impl CpuEncoderFrameRenderer {
    /// `cache_capacity` is the number of static subtrees kept rasterized between frames.
    pub fn new(
        cache_capacity: usize,
        input: &EncoderInput,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderEncodingError> {
        let pixmap = svgr::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| RenderEncodingError::CantAllocate("pixmap".to_owned()))?;

        Ok(Self {
            context: svgr::Context::new_from_pixmap_unsafe(&pixmap),
            pixmap,
            cache: SvgrCache::new(cache_capacity),
            pixmap_pool: PixmapPool::new(),
            converter: RgbaFrameConverter::for_input(input, width as i32, height as i32)?,
        })
    }
}

impl EncoderFrameRenderer for CpuEncoderFrameRenderer {
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: crate::Color,
    ) -> FFramesRendererResult<VideoFrame> {
        self.pixmap.fill(Color::from_rgba8(
            background.r,
            background.g,
            background.b,
            background.a,
        ));

        svgr::render(
            tree,
            super::fit_transform(tree, self.pixmap.width(), self.pixmap.height()),
            &mut self.pixmap.as_mut(),
            &mut self.cache,
            &self.pixmap_pool,
            &self.context,
        );

        self.converter
            .convert(self.pixmap.data())
            .map_err(|err| FFramesRendererError::from_chunk(0, err))
    }
}

impl FFramesRenderBackend for CpuRenderingBackend {
    fn frame_renderer(&self) -> Option<impl FrameRenderer + '_> {
        Some(super::CpuFrameRenderer::new(self.cache_capacity))
    }

    fn encoder_frame_renderer(
        &self,
        input: &EncoderInput,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<impl EncoderFrameRenderer + '_> {
        CpuEncoderFrameRenderer::new(self.cache_capacity, input, width, height)
            .map_err(|err| FFramesRendererError::from_chunk(0, err))
    }

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        render_options: &RenderOptions<'a, 'media>,
        font_db: &'a fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a crate::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()> {
        let output = output.as_ref();
        let extension = output
            .extension()
            .ok_or(FFramesRendererError::InvalidOutput)?;

        let session = Uuid::new_v4();
        let tmp_path = std::env::temp_dir().join(format!("fframes-{session}"));
        let directory = render_options.tmp_files_directory.unwrap_or(&tmp_path);
        if !directory.exists() {
            std::fs::create_dir(directory)?;
        }

        // The scheduler and segments work in output frames; `frame_offset` maps them back to
        // video frames when only a range is rendered.
        let frame_range = render_options.output_frame_range(ctx.duration_in_frames);
        let frame_offset = frame_range.start;

        let video_size = &ctx.current_video_size;
        let scheduler = FrameScheduler::new(
            frame_range.len(),
            self.concurrency,
            render_options
                .video_encoder_options
                .min_segment_frames(ctx.time_base.fps),
        );
        let writer = SegmentWriter::new(
            directory,
            extension.to_string_lossy().as_ref(),
            (
                video_size.width as i32,
                video_size.height as i32,
                ctx.time_base.fps as i32,
            ),
            render_options,
            &logger,
        );
        let encoder_input = writer
            .encoder_info()
            .and_then(|encoder| self.negotiate_encoder_input(&encoder))
            .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;
        let writer = writer.with_encoder_input(encoder_input);
        let failed = AtomicBool::new(false);
        // The audio mix does not depend on the frames: it is encoded into its own file
        // while they render and muxed in at the end.
        let audio_file = timeline
            .audio_map
            .as_ref()
            .map(|_| directory.join(format!("audio.{}", extension.to_string_lossy())));

        let render_worker = |worker: usize| -> FFramesRendererResult<()> {
            let worker_local_decoders = VideoDecodersWorker::new(1);
            let break_lines_cache = TextCache::new(self.text_cache_capacity);
            let mut converter_cache = usvgr::Cache::new_with_text_cache(self.text_cache_capacity);
            let mut renderer = self.encoder_frame_renderer(
                writer.encoder_input(),
                video_size.width as u32,
                video_size.height as u32,
            )?;

            let mut rendered_frames = 0;
            while let Some(claim) = scheduler.claim(worker) {
                if failed.load(Ordering::Relaxed) {
                    return Ok(());
                }
                if ctx.abort_signal.is_some_and(AbortSignal::is_aborted) {
                    return Err(FFramesRendererError::Aborted);
                }

                let video_frame = claim.frame + frame_offset;
                let svg = super::render_frame_guarded(
                    video,
                    Frame::__internal_make_for_renderer(
                        video_frame,
                        video_frame,
                        ctx.time_base.fps,
                        break_lines_cache.clone(),
                        worker_local_decoders.clone(),
                    ),
                    ctx,
                )?;

                let rtree = svg.into_svg_tree(usvg_options, &mut converter_cache, font_db)?;

                let frame = renderer.render_tree(&rtree, TVideo::BACKGROUND_COLOR)?;

                writer
                    .submit_frame(claim, frame)
                    .map_err(|err| FFramesRendererError::RenderChunkError(worker, err))?;
                logger.log_frame(rendered_frames, worker);
                rendered_frames += 1;
            }

            Ok(())
        };

        std::thread::scope(|scope| {
            let audio = audio_file.as_ref().zip(timeline.audio_map.as_ref()).map(
                |(audio_file, audio_map)| {
                    let (failed, logger) = (&failed, &logger);
                    scope.spawn(move || {
                        let result = unsafe {
                            concatenator::encode_audio_file(
                                audio_file,
                                audio_map,
                                render_options,
                                ctx,
                                logger,
                            )
                        }
                        .map_err(FFramesRendererError::ConcatChunkError);
                        if result.is_err() {
                            failed.store(true, Ordering::Relaxed);
                        }
                        result
                    })
                },
            );
            let workers: Vec<_> = (0..scheduler.workers())
                .map(|worker| {
                    let render_worker = &render_worker;
                    let failed = &failed;
                    scope.spawn(move || {
                        let result = render_worker(worker);
                        if result.is_err() {
                            failed.store(true, Ordering::Relaxed);
                        }
                        result
                    })
                })
                .collect();

            workers
                .into_iter()
                .chain(audio)
                .map(|worker| {
                    worker.join().map_err(|_| {
                        FFramesRendererError::Internal("Rendering thread panicked".to_owned())
                    })?
                })
                .collect::<FFramesRendererResult<Vec<_>>>()
        })?;

        let files = writer
            .finish()
            .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;

        unsafe {
            match &audio_file {
                Some(audio_file) => concatenator::concat_video_files_with_audio_file(
                    files.as_slice(),
                    audio_file,
                    output,
                    render_options,
                ),
                None => concatenator::concat_video_files_with_audio(
                    files.as_slice(),
                    output,
                    None,
                    render_options,
                    ctx,
                    &logger,
                ),
            }
            .map_err(FFramesRendererError::ConcatChunkError)?;
        }

        logger.success(output, Some(directory));
        Ok(())
    }

    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: crate::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: crate::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let mut converter_cache = usvgr::Cache::default();
        let rtree = super::render_frame_guarded(video, frame, &ctx)?.into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let frame = super::CpuFrameRenderer::new(0).render_tree(
            &rtree,
            TVideo::BACKGROUND_COLOR,
            ctx.current_video_size.width as u32,
            ctx.current_video_size.height as u32,
        )?;

        Ok(frame.pixels)
    }
}
