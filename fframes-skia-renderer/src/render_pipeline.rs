use fframes::usvgr::WriteOptions;
use fframes::{
    usvgr, AudioTimelineSamples, BreaksLruCache, FFramesContext, ResolvedRenderingTimeline, Video,
};
use fframes_renderer::{get_thread_count, EncoderOptions};
use fframes_renderer::{
    Encoder, EncoderFrame, FFramesLogger, FFramesRendererError, FFramesRendererResult,
    RenderEncodingResult,
};
use skia_safe::svg::Dom;
use skia_safe::{ConditionallySend, Sendable};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

use crate::resource_provider::SkiaFFramesProvider;
use crate::SkiaFFramesRenderer;

// Define different data layouts for each pipeline stage
struct FrameRequest {
    frame: usize,
    dom: Sendable<Dom>,
}

#[derive(Debug, Clone, Default)]
struct RenderPayload {
    frame_index: usize,
    pixels: Vec<u8>,
}

/// Configure skia backpressure render pipeline.
#[derive(Debug, Clone, Copy)]
pub struct SkiaPipelineConfig {
    /// The queue size holding the frames needs to be rendered and encoded.
    /// Bigger number means more frames can be buffered,
    /// but significantly increases memory consumption.
    pub buffer_queue_size: usize,
    /// Amount of threads available for encoding the video. Skia pipeline usually uses 3 threads
    /// for rendering, so by default all the other system threads will be used for encoding.
    /// Ignored for hardware accelerated encoders.
    pub encoder_threads: usize,
}

impl Default for SkiaPipelineConfig {
    fn default() -> Self {
        Self {
            buffer_queue_size: 10,
            encoder_threads: get_thread_count(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn start<'b, 'a, 'media: 'a, TVideo: Video + Sync + Send>(
    config: SkiaPipelineConfig,
    skia: SkiaFFramesRenderer,
    provider: SkiaFFramesProvider,
    output: &str,
    video: &'a TVideo,
    logger: Arc<dyn FFramesLogger>,
    usvg_options: &'a usvgr::Options,
    encoder_options: &EncoderOptions<'a>,
    font_db: &'a usvgr::fontdb::Database,
    timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
    ctx: &'a FFramesContext<'a, 'media>,
) -> FFramesRendererResult<()> {
    let (frame_tx, frame_receiver) = mpsc::sync_channel::<FrameRequest>(config.buffer_queue_size);
    let (render_sender, render_receiver) =
        thingbuf::mpsc::blocking::channel(config.buffer_queue_size);

    let encoder = unsafe {
        Encoder::new(
            /* with audio */ timeline.audio_map.is_some(),
            ctx.current_video_size.width as i32,
            ctx.current_video_size.height as i32,
            ctx.time_base.fps as i32,
            output,
            encoder_options,
            &logger.clone(),
        )
    }
    .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;

    // because total parallelism of audio and video encoding is not safe with ffmpeg
    // we give all the possible resources to the video encoder by default
    encoder
        .video_stream
        .set_encoder_threads_count(config.encoder_threads.saturating_sub(2));

    thread::scope(|scope| -> FFramesRendererResult<()> {
        scope
            .spawn(|| spawn_frame_generator(video, usvg_options, provider, font_db, frame_tx, ctx));

        scope.spawn(|| unsafe { spawn_video_encoder(render_receiver, &encoder, logger.clone()) });

        // execute skia stuff on the main thread to avoid sharing gpu context
        spawn_renderer(skia, frame_receiver, render_sender)?;

        Ok(())
    })?;

    if let Some(audio_stream) = encoder.audio_stream.as_ref() {
        // audio encoding will happen after the main pipeline so give it all we have
        audio_stream.set_encoder_threads_count(config.encoder_threads);
        unsafe { encoder.fill_audio_stream(timeline.audio_map.as_ref(), ctx) }
            .map_err(|err| FFramesRendererError::RenderChunkError(0, err))?;
    }

    logger.success(output, None);
    Ok(())
}

fn spawn_frame_generator<'a, 'media: 'a, TVideo: Video + Sync + Send>(
    video: &'a TVideo,
    usvg_options: &'a usvgr::Options<'a>,
    provider: SkiaFFramesProvider,
    font_db: &'a usvgr::fontdb::Database,
    frame_sender: SyncSender<FrameRequest>,
    ctx: &'a FFramesContext<'a, 'media>,
) -> FFramesRendererResult<()> {
    let break_lines_cache = BreaksLruCache::new(10); // FIXME add option
    let mut converter_cache = usvgr::Cache::new_with_text_cache(10);

    for frame in 0..ctx.duration_in_frames {
        let fframe = fframes::Frame::new_renderer(
            frame,
            frame,
            ctx.time_base.fps,
            break_lines_cache.clone(),
            provider.worker_local_decoders.clone(),
        );

        let svg = {
            video
                .render_frame(fframe, ctx)
                .into_svg_tree(usvg_options, &mut converter_cache, font_db)?
                .to_string(&WriteOptions::default())
        };

        let dom = Dom::from_str(&svg, provider.clone())
            .unwrap()
            .wrap_send()
            .map_err(|_| {
                FFramesRendererError::Custom("Failed to send DOM to renderer".to_string())
            })?;

        let request = FrameRequest { frame, dom };

        frame_sender.send(request).map_err(|_| {
            FFramesRendererError::Custom("Frame generator channel closed".to_string())
        })?;
    }

    Ok(())
}

fn spawn_renderer(
    skia: SkiaFFramesRenderer,
    frame_receiver: Receiver<FrameRequest>,
    render_sender: thingbuf::mpsc::blocking::Sender<RenderPayload>,
) -> FFramesRendererResult<()> {
    let mut surface = skia.surface;
    let mut gpu_context = skia.gpu_context;

    let image_info = surface.image_info();
    let compute_byte_size = image_info.compute_byte_size(image_info.min_row_bytes());

    while let Ok(FrameRequest { dom, frame }) = frame_receiver.recv() {
        if let Ok(mut payload) = render_sender.send_ref() {
            payload.frame_index = frame;
            // we reuse the buffers for messages to avoid allocations
            // so this is a way to initially allocate the buffer of required size
            // ideally to make this statically allocated on a stack
            if payload.pixels.len() != compute_byte_size {
                payload.pixels = vec![0; compute_byte_size];
            }

            let dom = dom.into_inner();
            dom.render(surface.canvas());

            drop(dom);

            if let Some(gpu_context) = gpu_context.as_mut() {
                gpu_context.flush_submit_and_sync_cpu();
            }

            let image = surface.image_snapshot();
            let pixmap = skia_safe::Pixmap::new(
                &image_info,
                &mut payload.pixels,
                image_info.min_row_bytes(),
            )
            .ok_or_else(|| FFramesRendererError::Custom("Failed to create pixmap".to_string()))?;

            if !image.read_pixels_to_pixmap_with_context(
                gpu_context.as_mut(),
                &pixmap,
                (0, 0),
                skia_safe::image::CachingHint::Disallow,
            ) {
                return Err(FFramesRendererError::Custom(
                    "Failed to read pixels from Skia image".to_string(),
                ));
            }
        }
    }

    Ok(())
}

unsafe fn spawn_video_encoder<'a, 'media: 'a>(
    render_receiver: thingbuf::mpsc::blocking::Receiver<RenderPayload>,
    encoder: &Encoder,
    logger: Arc<dyn FFramesLogger>,
) -> RenderEncodingResult<()> {
    let mut frame = EncoderFrame::new(&encoder.video_stream)?;

    while let Some(request) = render_receiver.recv_ref() {
        logger.log_frame(request.frame_index, 0);
        frame.fill_from_rgba_pixmap(request.frame_index as i64, &request.pixels);

        encoder.send_frame(&encoder.video_stream, &frame)?;
    }

    Ok(())
}
