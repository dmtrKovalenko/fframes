use crate::SkiaBackend;
use fframes::get_thread_count;
use fframes::{
    AudioTimelineSamples, FFramesContext, FrameClaim, FrameScheduler, RenderOptions,
    ResolvedRenderingTimeline, SegmentWriter, TextCache, Video, VideoDecodersWorker, usvgr,
};
use fframes::{FFramesLogger, FFramesRendererError, FFramesRendererResult, concatenator};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
#[cfg(feature = "debug")]
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum SkiaPipelineConcurrencyPolicy {
    /// Renders on as many GPU contexts as a third of the available threads. Every
    /// context records and submits its frames in parallel, which keeps the GPU busy while
    /// other contexts wait for their frames.
    MaxPerformance,
    /// Render on the given number of GPU contexts.
    /// Make sure to always measure the performance on the final hardware to find the best value.
    Concurrency(usize),
    /// Render on a single GPU context.
    OnePipeline,
}

/// Pipeline configuration
#[derive(Debug, Clone, Copy)]
pub struct SkiaPipelineConfig {
    /// How many frames can wait between the pipeline stages.
    pub buffer_queue_size: usize,
    /// The number of threads that build frame trees and encode the rendered frames.
    pub encoder_threads: usize,
    pub concurrency_policy: SkiaPipelineConcurrencyPolicy,
}

impl Default for SkiaPipelineConfig {
    fn default() -> Self {
        Self {
            buffer_queue_size: 10,
            encoder_threads: get_thread_count(),
            concurrency_policy: SkiaPipelineConcurrencyPolicy::OnePipeline,
        }
    }
}

impl SkiaPipelineConfig {
    pub(crate) fn gpu_contexts(&self) -> usize {
        match self.concurrency_policy {
            SkiaPipelineConcurrencyPolicy::MaxPerformance => get_thread_count() / 3,
            SkiaPipelineConcurrencyPolicy::Concurrency(contexts) => contexts,
            SkiaPipelineConcurrencyPolicy::OnePipeline => 1,
        }
        .max(1)
    }
}

pub(crate) struct Pipeline<'p, 'a, 'media, TVideo: Video + Sync + Send, TBackend: SkiaBackend> {
    pub(crate) ctx: &'a FFramesContext<'a, 'media>,
    pub(crate) render_options: &'a RenderOptions<'a, 'media>,
    pub(crate) font_db: &'a usvgr::fontdb::Database,
    pub(crate) logger: Arc<dyn FFramesLogger>,
    pub(crate) output: &'p Path,
    pub(crate) pipeline_config: SkiaPipelineConfig,
    pub(crate) skia: &'p TBackend,
    pub(crate) timeline: &'a ResolvedRenderingTimeline<'a, AudioTimelineSamples>,
    pub(crate) usvg_options: &'a usvgr::Options<'a>,
    pub(crate) video: &'a TVideo,
    pub(crate) background_color: skia_safe::Color,
}

struct RenderedFrame {
    claim: FrameClaim,
    pixels: Vec<u8>,
}

/// Recycled frame buffers, a 1080p frame is 8MB and allocating it every frame is not free.
#[derive(Default)]
struct BufferPool(Mutex<Vec<Vec<u8>>>);

impl BufferPool {
    fn take(&self, size: usize) -> Vec<u8> {
        self.0
            .lock()
            .unwrap()
            .pop()
            .filter(|buffer| buffer.len() == size)
            .unwrap_or_else(|| vec![0; size])
    }

    fn give_back(&self, buffers: impl IntoIterator<Item = Vec<u8>>) {
        self.0.lock().unwrap().extend(buffers);
    }
}

/// Renders a video through three pools of threads connected by bounded queues:
///
/// ```text
/// generators (Video::render_frame + usvgr tree) ──► GPU contexts (draw + readback)
///                                                        │
///       segment files ◄── SegmentWriter ◄── encoders ◄───┘
/// ```
///
/// The [`FrameScheduler`] gives every generator a contiguous range of frames that is
/// encoded as a separate segment, the segments are concatenated with the audio at the end.
pub(crate) fn render<'p, 'a, 'media: 'a, TVideo: Video + Sync + Send, TBackend: SkiaBackend>(
    Pipeline {
        ctx,
        render_options,
        font_db,
        logger,
        output,
        pipeline_config,
        skia,
        timeline,
        usvg_options,
        video,
        background_color,
    }: Pipeline<'p, 'a, 'media, TVideo, TBackend>,
) -> FFramesRendererResult<()> {
    let workers = pipeline_config.encoder_threads.max(1);
    let gpu_contexts = pipeline_config.gpu_contexts();
    let queue_size = pipeline_config.buffer_queue_size.max(1);

    let extension = output
        .extension()
        .ok_or(FFramesRendererError::InvalidOutput)?
        .to_string_lossy()
        .into_owned();
    let tmp_path = std::env::temp_dir().join(format!("fframes-skia-{}", uuid::Uuid::new_v4()));
    let directory = render_options.tmp_files_directory.unwrap_or(&tmp_path);
    if !directory.exists() {
        std::fs::create_dir(directory)?;
    }

    // Generators own the segments, so their number also bounds how many segments are
    // encoded at once. Half of the threads is plenty to feed the GPU and leaves the rest
    // to the encoders.
    let generators = (workers / 2).max(1);
    // The scheduler and segments work in output frames; `frame_offset` maps them back to
    // video frames when only a range is rendered.
    let frame_range = render_options.output_frame_range(ctx.duration_in_frames);
    let frame_offset = frame_range.start;
    let scheduler = FrameScheduler::new(
        frame_range.len(),
        generators,
        render_options
            .video_encoder_options
            .min_segment_frames(ctx.time_base.fps),
    );
    let writer = SegmentWriter::new(
        directory,
        &extension,
        (
            ctx.current_video_size.width as i32,
            ctx.current_video_size.height as i32,
            ctx.time_base.fps as i32,
        ),
        render_options,
        &logger,
    );

    #[cfg(feature = "debug")]
    let metrics = crate::metrics::PipelineMetrics::new(generators, gpu_contexts, workers);

    let failed = AtomicBool::new(false);
    let buffers = BufferPool::default();
    let (tree_sender, tree_receiver) = mpsc::sync_channel::<(FrameClaim, usvgr::Tree)>(queue_size);
    let (frame_sender, frame_receiver) = mpsc::sync_channel::<RenderedFrame>(queue_size);
    let tree_receiver = Mutex::new(tree_receiver);
    let frame_receiver = Mutex::new(frame_receiver);

    let results = thread::scope(|scope| {
        let mark_failed = |result: FFramesRendererResult<()>| {
            if result.is_err() {
                failed.store(true, Ordering::Relaxed);
            }
            result
        };

        let mut handles = Vec::new();
        for worker in 0..scheduler.workers() {
            let tree_sender = tree_sender.clone();
            let (scheduler, failed) = (&scheduler, &failed);
            #[cfg(feature = "debug")]
            let metrics = metrics.generator_metrics.clone();
            handles.push(scope.spawn(move || {
                mark_failed(generate_frames(
                    worker,
                    video,
                    scheduler,
                    frame_offset,
                    usvg_options,
                    font_db,
                    tree_sender,
                    queue_size,
                    ctx,
                    failed,
                    #[cfg(feature = "debug")]
                    metrics,
                ))
            }));
        }
        drop(tree_sender);

        for _ in 0..gpu_contexts {
            let frame_sender = frame_sender.clone();
            let (tree_receiver, buffers, failed, logger) =
                (&tree_receiver, &buffers, &failed, &logger);
            #[cfg(feature = "debug")]
            let metrics = metrics.renderer_metrics.clone();
            handles.push(scope.spawn(move || {
                mark_failed(render_frames(
                    skia,
                    logger,
                    tree_receiver,
                    frame_sender,
                    buffers,
                    ctx,
                    failed,
                    background_color,
                    #[cfg(feature = "debug")]
                    metrics,
                ))
            }));
        }
        drop(frame_sender);

        for _ in 0..scheduler.workers() {
            let (frame_receiver, buffers, failed, writer) =
                (&frame_receiver, &buffers, &failed, &writer);
            #[cfg(feature = "debug")]
            let metrics = metrics.encoder_metrics.clone();
            handles.push(scope.spawn(move || {
                mark_failed(encode_frames(
                    frame_receiver,
                    writer,
                    buffers,
                    failed,
                    #[cfg(feature = "debug")]
                    metrics,
                ))
            }));
        }

        handles
            .into_iter()
            .map(|handle| {
                handle.join().map_err(|e| {
                    FFramesRendererError::Internal(format!("Rendering thread panicked: {e:?}"))
                })?
            })
            .collect::<Vec<_>>()
    });

    // report the error that stopped the pipeline rather than the ones it caused
    if let Some(err) = results.into_iter().filter_map(Result::err).min_by_key(
        |err| matches!(err, FFramesRendererError::Custom(message) if message.contains("closed")),
    ) {
        return Err(err);
    }

    #[cfg(feature = "debug")]
    metrics.print_stats();

    let files = writer
        .finish()
        .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;

    unsafe {
        concatenator::concat_video_files_with_audio(
            files.as_slice(),
            output,
            timeline.audio_map.as_ref(),
            render_options,
            ctx,
            &logger,
        )
        .map_err(FFramesRendererError::ConcatChunkError)?;
    }

    logger.success(output, Some(directory));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn generate_frames<'a, 'media: 'a, TVideo: Video + Sync + Send>(
    worker: usize,
    video: &'a TVideo,
    scheduler: &FrameScheduler,
    frame_offset: usize,
    usvg_options: &'a usvgr::Options<'a>,
    font_db: &'a usvgr::fontdb::Database,
    tree_sender: SyncSender<(FrameClaim, usvgr::Tree)>,
    queue_size: usize,
    ctx: &'a FFramesContext<'a, 'media>,
    failed: &AtomicBool,
    #[cfg(feature = "debug")] metrics: Arc<crate::metrics::ThreadMetrics>,
) -> FFramesRendererResult<()> {
    let break_lines_cache = TextCache::new(10);
    let mut converter_cache = usvgr::Cache::new_with_text_cache(10);
    // x2 because sometimes we might need to decode 2 frames at once
    let video_decoders_worker = VideoDecodersWorker::new(queue_size * 2);

    while let Some(claim) = scheduler.claim(worker) {
        #[cfg(feature = "debug")]
        let start = Instant::now();

        if failed.load(Ordering::Relaxed) {
            return Ok(());
        }
        if ctx
            .abort_signal
            .is_some_and(fframes::AbortSignal::is_aborted)
        {
            return Err(FFramesRendererError::Aborted);
        }

        let video_frame = claim.frame + frame_offset;
        let frame = fframes::Frame::__internal_make_for_renderer(
            video_frame,
            video_frame,
            ctx.time_base.fps,
            break_lines_cache.clone(),
            video_decoders_worker.clone(),
        );

        // Direct path: Svgr -> usvgr::Tree (no string serialization, no Dom parsing)
        let tree = fframes::render_frame_guarded(video, frame, ctx)?.into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        #[cfg(feature = "debug")]
        {
            metrics
                .total_time
                .fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
            metrics.items_processed.fetch_add(1, Ordering::Relaxed);
        }

        tree_sender.send((claim, tree)).map_err(|_| {
            FFramesRendererError::Custom("Frame generator channel closed".to_string())
        })?;
    }

    Ok(())
}

fn receive<T>(receiver: &Mutex<Receiver<T>>) -> Option<T> {
    receiver.lock().unwrap().recv().ok()
}

#[allow(clippy::too_many_arguments)]
fn render_frames<TBackend: SkiaBackend>(
    backend: &TBackend,
    logger: &Arc<dyn FFramesLogger>,
    tree_receiver: &Mutex<Receiver<(FrameClaim, usvgr::Tree)>>,
    frame_sender: SyncSender<RenderedFrame>,
    buffers: &BufferPool,
    ctx: &FFramesContext,
    failed: &AtomicBool,
    background_color: skia_safe::Color,
    #[cfg(feature = "debug")] metrics: Arc<crate::metrics::ThreadMetrics>,
) -> FFramesRendererResult<()> {
    // Scaled renders (`scale_resolution`) need a surface of the output size, not the one the
    // backend was created with.
    let output_size = (
        ctx.current_video_size.width as i32,
        ctx.current_video_size.height as i32,
    );
    let (mut surface, mut gpu_context) =
        crate::surface_with_size(backend, output_size.0, output_size.1)?;
    let image_info = surface.image_info();
    let frame_size = image_info.compute_byte_size(image_info.min_row_bytes());
    let row_bytes = image_info.min_row_bytes();

    // Persist across frames so static paths/images are converted only once
    let mut render_cache = crate::render::RenderCache::new();

    while let Some((claim, tree)) = {
        #[cfg(feature = "debug")]
        let wait_start = Instant::now();
        let request = receive(tree_receiver);
        #[cfg(feature = "debug")]
        metrics
            .channel_wait_time
            .fetch_add(wait_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        request
    } {
        if failed.load(Ordering::Relaxed) {
            return Ok(());
        }
        if ctx
            .abort_signal
            .is_some_and(fframes::AbortSignal::is_aborted)
        {
            return Err(FFramesRendererError::Aborted);
        }

        #[cfg(feature = "debug")]
        let start = Instant::now();

        let mut pixels = buffers.take(frame_size);
        let pixmap = skia_safe::Pixmap::new(&image_info, &mut pixels, row_bytes)
            .ok_or_else(|| FFramesRendererError::Custom("Failed to create pixmap".to_string()))?;

        let canvas = surface.canvas();
        canvas.clear(background_color);
        canvas.save();
        crate::apply_fit(canvas, &tree, output_size.0, output_size.1);
        crate::render::render_tree(&tree, canvas, &mut render_cache);
        canvas.restore();

        if let Some(gpu_context) = gpu_context.as_mut() {
            gpu_context.flush_submit_and_sync_cpu();
        }

        let image = surface.image_snapshot();
        if !image.read_pixels_to_pixmap_with_context(
            gpu_context.as_mut(),
            &pixmap,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        ) {
            return Err(FFramesRendererError::Custom(
                "Failed to read pixels from Skia image".to_string(),
            ));
        };

        // Video-frame images view the tree's pixel buffers without
        // copying, so the tree has to outlive the flush and readback.
        drop(tree);
        logger.log_frame(claim.frame, 0);

        #[cfg(feature = "debug")]
        {
            metrics
                .total_time
                .fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
            metrics.items_processed.fetch_add(1, Ordering::Relaxed);
        }

        frame_sender
            .send(RenderedFrame { claim, pixels })
            .map_err(|_| FFramesRendererError::Custom("Renderer channel closed".to_string()))?;
    }

    Ok(())
}

fn encode_frames(
    frame_receiver: &Mutex<Receiver<RenderedFrame>>,
    writer: &SegmentWriter,
    buffers: &BufferPool,
    failed: &AtomicBool,
    #[cfg(feature = "debug")] metrics: Arc<crate::metrics::ThreadMetrics>,
) -> FFramesRendererResult<()> {
    while let Some(RenderedFrame { claim, pixels }) = {
        #[cfg(feature = "debug")]
        let wait_start = Instant::now();
        let request = receive(frame_receiver);
        #[cfg(feature = "debug")]
        metrics
            .channel_wait_time
            .fetch_add(wait_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        request
    } {
        if failed.load(Ordering::Relaxed) {
            return Ok(());
        }

        #[cfg(feature = "debug")]
        let start = Instant::now();

        let released = writer
            .submit_owned(claim, pixels)
            .map_err(|err| FFramesRendererError::RenderChunkError(claim.segment, err))?;
        buffers.give_back(released);

        #[cfg(feature = "debug")]
        {
            metrics
                .total_time
                .fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
            metrics.items_processed.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(())
}
