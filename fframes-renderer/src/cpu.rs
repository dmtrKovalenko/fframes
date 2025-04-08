use crate::{
    RenderEncodingResult, get_thread_count, render_backend::FFramesRenderBackend,
    renderer_error::RenderEncodingError,
};
use fframes::{
    AudioTimelineSamples, BreaksLruCache, Frame, ResolvedRenderingTimeline, Video,
    VideoDecodersWorker, usvgr,
};
use rayon::prelude::*;
use std::{path::Path, sync::Arc};
use svgr::{PixmapPool, SvgrCache};
use usvgr::fontdb;
use uuid::Uuid;

use crate::{
    concatenator,
    encoder::{Encoder, EncoderOptions},
    encoder_frame::EncoderFrame,
    fframes_logger::FFramesLogger,
    renderer_error::{FFramesRendererError, FFramesRendererResult},
};

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

impl FFramesRenderBackend for CpuRenderingBackend {
    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        encoder_options: &'a EncoderOptions<'a>,
        font_db: &'a fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()> {
        let output = output.as_ref();
        let extension = output
            .extension()
            .ok_or(FFramesRendererError::InvalidOutput)?;

        let session = Uuid::new_v4();
        let tmp_path = std::env::temp_dir().join(format!("fframes-{session}"));
        let directory = encoder_options.tmp_files_directory.unwrap_or(&tmp_path);

        if !directory.exists() {
            std::fs::create_dir(directory)?;
        }

        let files = encoder_options
            .split_video_chunks(ctx.duration_in_frames, self.concurrency)
            .par_iter()
            .enumerate()
            .map(|(thread_number, chunk_range)| {
                let file = directory.join(format!(
                    "{thread_number}.{}",
                    extension.to_string_lossy().as_ref(),
                ));

                unsafe {
                    Encoder::with_output(
                        /* with audio */ false,
                        ctx.current_video_size.width as i32,
                        ctx.current_video_size.height as i32,
                        ctx.time_base.fps as i32,
                        &file,
                        encoder_options,
                        &logger,
                        &mut |encoder| {
                            let mut frame = EncoderFrame::new(&encoder.video_stream)?;

                            let pixmap_pool = PixmapPool::new();
                            let worker_local_decoders = VideoDecodersWorker::new(1);
                            let mut svgr_cache = SvgrCache::new(self.cache_capacity);
                            let break_lines_cache = BreaksLruCache::new(self.text_cache_capacity);
                            let mut converter_cache =
                                usvgr::Cache::new_with_text_cache(self.text_cache_capacity);

                            let mut pixmap = svgr::tiny_skia::Pixmap::new(
                                ctx.current_video_size.width as u32,
                                ctx.current_video_size.height as u32,
                            )
                            .ok_or_else(|| {
                                RenderEncodingError::CantAllocate("pixmap".to_owned())
                            })?;

                            let svgr_ctx = svgr::Context::new_from_pixmap_unsafe(&pixmap);
                            chunk_range.to_owned().enumerate().try_for_each(
                                |(index, fr)| -> RenderEncodingResult<()> {
                                    pixmap.fill(svgr::tiny_skia::Color::BLACK);

                                    let svg = video.render_frame(
                                        Frame::__internal_make_for_renderer(
                                            fr,
                                            fr,
                                            ctx.time_base.fps,
                                            break_lines_cache.clone(),
                                            worker_local_decoders.clone(),
                                        ),
                                        ctx,
                                    );

                                    let rtree = svg.into_svg_tree(
                                        usvg_options,
                                        &mut converter_cache,
                                        font_db,
                                    )?;

                                    svgr::render(
                                        &rtree,
                                        svgr::tiny_skia::Transform::default(),
                                        &mut pixmap.as_mut(),
                                        &mut svgr_cache,
                                        &pixmap_pool,
                                        &svgr_ctx,
                                    );

                                    frame.fill_from_rgba_pixmap(pixmap.data());
                                    // we use frame indexes as pts which if fine because the av_packet_rescale_ts will
                                    // automatically convert it to the correct timebase
                                    frame.set_pts(fr as i64);
                                    encoder.send_frame(&encoder.video_stream, &frame)?;
                                    logger.log_frame(index, thread_number);

                                    Ok(())
                                },
                            )?;

                            encoder.submit_leftover_b_frames(
                                &frame,
                                &encoder.video_stream,
                                chunk_range.end - chunk_range.start,
                            )?;

                            Ok(())
                        },
                    )
                }
                .map_err(|av_err| FFramesRendererError::RenderChunkError(thread_number, av_err))?;

                Ok(file)
            })
            .collect::<FFramesRendererResult<Vec<_>>>()?;

        unsafe {
            concatenator::concat_video_files_with_audio(
                files.as_slice(),
                output,
                self.concurrency as i32,
                timeline.audio_map.as_ref(),
                encoder_options,
                ctx,
            )
            .map_err(FFramesRendererError::ConcatChunkError)?;
        }

        logger.success(output, Some(directory));
        Ok(())
    }

    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let mut pixmap = svgr::tiny_skia::Pixmap::new(ctx.current_video_size.width as u32, ctx.current_video_size.height as u32)
            .ok_or_else(|| FFramesRendererError::Internal("Failed to allocate pixmap for rendering. This may indicate that this machine is out of memory.".to_owned()))?;

        let mut converter_cache = usvgr::Cache::default();
        let rtree = video.render_frame(frame, &ctx).into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let ctx = svgr::Context::new_from_pixmap_unsafe(&pixmap);
        svgr::render(
            &rtree,
            svgr::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
            &mut SvgrCache::none(),
            &PixmapPool::new(),
            &ctx,
        );

        Ok(pixmap.take())
    }
}
