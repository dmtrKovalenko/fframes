use crate::SkiaBackend;
use fframes::get_thread_count;
use fframes::{
    usvgr, AudioTimelineSamples, FFramesContext, RenderOptions, ResolvedRenderingTimeline,
    TextCache, Video, VideoDecodersWorker,
};
use fframes::{
    Encoder, EncoderFrame, FFramesLogger, FFramesRendererError, FFramesRendererResult,
    RenderEncodingResult,
};
use std::ops::Range;
use std::path::Path;
#[cfg(feature = "debug")]
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;
#[cfg(feature = "debug")]
use std::time::Instant;

// Pipeline data structures - now sends the usvgr::Tree directly instead of a Skia Dom
struct FrameRequest {
    frame: usize,
    tree: usvgr::Tree,
}

impl Eq for FrameRequest {}
impl PartialEq for FrameRequest {
    fn eq(&self, other: &Self) -> bool {
        self.frame == other.frame
    }
}
impl PartialOrd for FrameRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FrameRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.frame.cmp(&other.frame)
    }
}

#[derive(Debug, Clone, Default)]
struct RenderPayload {
    frame_index: usize,
    pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum SkiaPipelineConcurrencyPolicy {
    /// If your machine has a lot of cores available we can maximize the performance by
    /// spawning as much concurrent pipelines as possible. Make sure that the gpu access
    /// is always limited, so it might be useful only in case your renderer worker is
    /// spending too much time waiting.
    MaxPerformance,
    /// Specify the number of concurrent pipelines to spawn.
    /// Make sure to always measure the performance on the final hardware to find the best value.
    Concurrency(usize),
    /// This uses as much threads one as one pipeline is taking based on your configuration
    OnePipeline,
}

/// Pipeline configuration
#[derive(Debug, Clone, Copy)]
pub struct SkiaPipelineConfig {
    pub buffer_queue_size: usize,
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

pub(crate) struct Pipeline<'b, 'a, 'media, TVideo: Video + Sync + Send, TBackend: SkiaBackend> {
    pub(crate) ctx: &'a FFramesContext<'a, 'media>,
    pub(crate) render_options: &'a RenderOptions<'a, 'media>,
    pub(crate) font_db: &'a usvgr::fontdb::Database,
    pub(crate) frame_range: &'b Range<usize>,
    pub(crate) include_audio: bool,
    pub(crate) logger: Arc<dyn FFramesLogger>,
    pub(crate) output: &'b Path,
    pub(crate) pipeline_config: SkiaPipelineConfig,
    pub(crate) skia: &'b TBackend,
    pub(crate) timeline: &'a ResolvedRenderingTimeline<'a, AudioTimelineSamples>,
    pub(crate) usvg_options: &'a usvgr::Options<'a>,
    pub(crate) video: &'a TVideo,
    pub(crate) background_color: skia_safe::Color,
}

pub fn start<'a, 'b, 'media: 'a, TVideo: Video + Sync + Send, TBackend: SkiaBackend>(
    Pipeline {
        ctx,
        render_options: encoder_options,
        font_db,
        frame_range,
        include_audio,
        logger,
        output,
        skia,
        timeline,
        pipeline_config,
        usvg_options,
        video,
        background_color,
    }: Pipeline<'a, 'b, 'media, TVideo, TBackend>,
) -> FFramesRendererResult<()> {
    let (render_sender, render_receiver) =
        thingbuf::mpsc::blocking::channel(pipeline_config.buffer_queue_size);
    let (frame_tx, frame_receiver) =
        mpsc::sync_channel::<FrameRequest>(pipeline_config.buffer_queue_size);

    let encoder = unsafe {
        Encoder::new(
            include_audio && timeline.audio_map.is_some(),
            ctx.current_video_size.width as i32,
            ctx.current_video_size.height as i32,
            ctx.time_base.fps as i32,
            output,
            encoder_options,
            &logger.clone(),
        )
    }
    .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;

    #[cfg(feature = "debug")]
    let metrics = Arc::new(crate::metrics::PipelineMetrics::new(1));

    thread::scope(|scope| -> FFramesRendererResult<()> {
        let generator_handle = scope.spawn(|| {
            spawn_frame_generator(
                video,
                frame_range.clone(),
                usvg_options,
                font_db,
                frame_tx,
                &pipeline_config,
                ctx,
                #[cfg(feature = "debug")]
                metrics.generator_metrics.clone(),
            )
        });

        let encoder_handle = scope.spawn(|| unsafe {
            spawn_video_encoder(
                render_receiver,
                &encoder,
                frame_range.end - frame_range.start,
                // we have to pass abort signals to all threads to not wait for the
                // buffer queue to be empty before being able to finish the pipeline
                ctx.abort_signal,
                #[cfg(feature = "debug")]
                metrics.encoder_metrics.clone(),
            )
        });

        spawn_renderer(
            skia,
            logger.clone(),
            frame_receiver,
            render_sender,
            ctx.abort_signal,
            background_color,
            #[cfg(feature = "debug")]
            metrics.renderer_metrics.clone(),
        )?;

        generator_handle.join().map_err(|e| {
            FFramesRendererError::Internal(format!("Frame generator thread panicked: {e:?}"))
        })??;
        encoder_handle
            .join()
            .map_err(|e| FFramesRendererError::Internal(format!("Encoder thread panicked: {e:?}")))?
            .map_err(|e| FFramesRendererError::RenderChunkError(0, e))?;

        Ok(())
    })?;

    if let Some(audio_stream) = encoder.audio_stream.as_ref() {
        audio_stream.set_encoder_threads_count(pipeline_config.encoder_threads);
        unsafe { encoder.fill_audio_stream(timeline.audio_map.as_ref(), ctx, &logger) }
            .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;
    }

    #[cfg(feature = "debug")]
    metrics.print_stats();

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn spawn_frame_generator<'a, 'media: 'a, TVideo: Video + Sync + Send>(
    video: &'a TVideo,
    frame_range: Range<usize>,
    usvg_options: &'a usvgr::Options<'a>,
    font_db: &'a usvgr::fontdb::Database,
    frame_sender: SyncSender<FrameRequest>,
    pipeline_config: &SkiaPipelineConfig,
    ctx: &'a FFramesContext<'a, 'media>,
    #[cfg(feature = "debug")] metrics: Arc<crate::metrics::ThreadMetrics>,
) -> FFramesRendererResult<()> {
    let break_lines_cache = TextCache::new(10);
    let mut converter_cache = usvgr::Cache::new_with_text_cache(10);

    // x2 because sometimes we might need to decode 2 frames at once
    let video_decoders_worker = VideoDecodersWorker::new(pipeline_config.buffer_queue_size * 2);

    for frame in frame_range {
        #[cfg(feature = "debug")]
        let start = Instant::now();

        if ctx
            .abort_signal
            .is_some_and(fframes::AbortSignal::is_aborted)
        {
            return Err(FFramesRendererError::Aborted);
        }

        let fframe = fframes::Frame::__internal_make_for_renderer(
            frame,
            frame,
            ctx.time_base.fps,
            break_lines_cache.clone(),
            video_decoders_worker.clone(),
        );

        // Direct path: Svgr -> usvgr::Tree (no string serialization, no Dom parsing)
        let tree = video.render_frame(fframe, ctx).into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        frame_sender
            .send(FrameRequest { frame, tree })
            .map_err(|_| {
                FFramesRendererError::Custom("Frame generator channel closed".to_string())
            })?;

        #[cfg(feature = "debug")]
        {
            let duration = start.elapsed();
            metrics
                .total_time
                .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
            metrics.items_processed.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(())
}

fn spawn_renderer<TBackend: SkiaBackend>(
    backend: &TBackend,
    logger: Arc<dyn FFramesLogger>,
    frame_receiver: Receiver<FrameRequest>,
    render_sender: thingbuf::mpsc::blocking::Sender<RenderPayload>,
    abort_signal: Option<&fframes::AbortSignal>,
    background_color: skia_safe::Color,
    #[cfg(feature = "debug")] metrics: Arc<crate::metrics::ThreadMetrics>,
) -> FFramesRendererResult<()> {
    let (mut surface, mut gpu_context) = backend.create_skia_surface()?;
    let image_info = surface.image_info();
    let frame_datavec_size = image_info.compute_byte_size(image_info.min_row_bytes());
    let row_bytes = image_info.min_row_bytes();

    // Persist across frames so static paths/images are converted only once
    let mut render_cache = crate::render::RenderCache::new();

    while let Ok(FrameRequest { tree, frame }) = {
        #[cfg(feature = "debug")]
        let wait_start = Instant::now();
        let result = frame_receiver.recv();
        #[cfg(feature = "debug")]
        {
            metrics
                .channel_wait_time
                .fetch_add(wait_start.elapsed().as_micros() as u64, Ordering::Release);
        }

        result
    } {
        if abort_signal.is_some_and(fframes::AbortSignal::is_aborted) {
            return Err(FFramesRendererError::Aborted);
        }

        #[cfg(feature = "debug")]
        let start = Instant::now();
        logger.log_frame(frame, 0);

        if let Ok(mut payload) = render_sender.send_ref() {
            payload.frame_index = frame;
            if payload.pixels.len() != frame_datavec_size {
                payload.pixels = vec![0; frame_datavec_size];
            }

            let pixmap = skia_safe::Pixmap::new(&image_info, &mut payload.pixels, row_bytes)
                .ok_or_else(|| {
                    FFramesRendererError::Custom("Failed to create pixmap".to_string())
                })?;

            surface.canvas().clear(background_color);

            // Direct canvas rendering: usvgr::Tree -> Skia Canvas (no string roundtrip)
            crate::render::render_tree(&tree, surface.canvas(), &mut render_cache);
            drop(tree);

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
        }

        #[cfg(feature = "debug")]
        {
            let duration = start.elapsed();
            metrics
                .total_time
                .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
            metrics.items_processed.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(())
}

unsafe fn spawn_video_encoder<'a, 'media: 'a>(
    render_receiver: thingbuf::mpsc::blocking::Receiver<RenderPayload>,
    encoder: &Encoder,
    frames_to_render: usize,
    abort_signal: Option<&fframes::AbortSignal>,
    #[cfg(feature = "debug")] metrics: Arc<crate::metrics::ThreadMetrics>,
) -> RenderEncodingResult<()> {
    unsafe {
        let mut frame = EncoderFrame::new(&encoder.video_stream)?;

        while let Some(request) = {
            #[cfg(feature = "debug")]
            let wait_start = Instant::now();
            let result = render_receiver.recv();
            #[cfg(feature = "debug")]
            {
                metrics
                    .channel_wait_time
                    .fetch_add(wait_start.elapsed().as_micros() as u64, Ordering::Release);
            }

            result
        } {
            if abort_signal.is_some_and(fframes::AbortSignal::is_aborted) {
                return Err(fframes::RenderEncodingError::Aborted);
            }

            #[cfg(feature = "debug")]
            let start = Instant::now();

            frame.set_pts(request.frame_index as i64);
            frame.fill_from_rgba_pixmap(&request.pixels);
            encoder.send_frame(&encoder.video_stream, &frame)?;

            #[cfg(feature = "debug")]
            {
                let duration = start.elapsed();
                metrics
                    .total_time
                    .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
                metrics.items_processed.fetch_add(1, Ordering::Relaxed);
            }
        }

        encoder.submit_leftover_b_frames(&frame, &encoder.video_stream, frames_to_render)?;

        Ok(())
    }
}
