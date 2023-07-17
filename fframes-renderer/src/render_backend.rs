use fframes::{
    frame, usvgr, video::Video, AudioTimelineSamples, BreaksLruCache, ResolvedRenderingTimeline,
};
use rayon::prelude::*;
use std::{ops::Range, sync::Arc};
use svgr::SvgrCache;
use usvgr_text_layout::{FontsCache, TreeTextToPath, UsvgrTextLayoutCache};
use uuid::Uuid;

use crate::{
    concatenator,
    encoder::{Encoder, EncoderOptions},
    encoder_frame::EncoderFrame,
    fframes_logger::FFramesLogger,
    renderer_error::{FFramesError, FFramesResult},
};

pub use crate::gpu::GpuRenderingBackend;

#[allow(clippy::too_many_arguments)]
pub trait FFramesRenderBackend {
    fn debug_frame<TVideo: Video + Sync + Sized>(
        &self,
        frame: fframes::Frame,
        out: &str,
        video: TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr_text_layout::fontdb::Database,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()>;

    fn render<'a, TVideo: Video + Sync + Sized>(
        &self,
        output: &'a str,
        video: TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &usvgr::Options,
        encoder_options: &EncoderOptions<'a>,
        font_db: &usvgr_text_layout::fontdb::Database,
        timeline: &ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()>
    where
        Self: Sized;
}

#[derive(Debug, Default)]
pub enum RenderBackendVariant {
    Gpu,
    #[default]
    Cpu,
}

impl RenderBackendVariant {
    pub fn make_backend(&self) -> impl FFramesRenderBackend {
        match self {
            RenderBackendVariant::Gpu => GpuRenderingBackend {
                ..Default::default()
            },
            RenderBackendVariant::Cpu => GpuRenderingBackend {
                ..Default::default()
            },
        }
    }
}

pub struct CpuRenderingBackend {
    /// The number of **individual svg elements or groups** to cache. It is important to understand that CPU
    /// rendering is very slow for mostly all the filters, shadows and gradients so this is important to reuse unchanged elements.
    /// But from the flip side do not set this to the unreasonably large values as it will consume a lot of memory for no reason.
    ///
    /// The optimal size = general number of static (not animating) elements in your video.
    ///
    /// @default 20
    pub cache_capacity: usize,
    /// The number of threads to use for rendering. By default it will use the number of logical cores on your machine.
    /// There is no reason to set this to a value greater than the number of logical cores because each thread will render its own video which after will be concatenated.
    ///
    /// @default rayon::current_num_threads()
    pub concurrency: usize,
    /// The number of frame.text_break_lines results to be cached.
    /// Text rendering and wrapping is very expensive especially on CPU as it involves a lot of text shaping and layout along with font resolution.
    pub text_cache_capacity: usize,
}

impl Default for CpuRenderingBackend {
    fn default() -> Self {
        Self {
            cache_capacity: 20,
            text_cache_capacity: 10,
            concurrency: rayon::current_num_threads(),
        }
    }
}

fn divide_round_up(a: usize, b: usize) -> usize {
    (a + (b - 1)) / b
}

impl CpuRenderingBackend {
    fn split_video_chunks(&self, duration_in_frames: usize) -> Vec<Range<usize>> {
        let chunk_size = divide_round_up(duration_in_frames, self.concurrency);
        let mut chunks = vec![];
        let mut prev_chunk = 0;

        while prev_chunk < duration_in_frames {
            if duration_in_frames - prev_chunk > chunk_size {
                chunks.push(prev_chunk..prev_chunk + chunk_size);
                prev_chunk += chunk_size;
            } else {
                let last_chunk = duration_in_frames - prev_chunk;
                chunks.push(prev_chunk..prev_chunk + last_chunk);
                prev_chunk += last_chunk;
            }
        }

        chunks
    }
}

impl FFramesRenderBackend for CpuRenderingBackend {
    fn render<'a, TVideo: Video + Sync + Sized>(
        &self,
        output: &'a str,
        video: TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &usvgr::Options,
        encoder_options: &EncoderOptions<'a>,
        font_db: &usvgr_text_layout::fontdb::Database,
        timeline: &ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()> {
        let extension = output
            .split('.')
            .last()
            .ok_or(FFramesError::InvalidOutput)?;

        let session = Uuid::new_v4();
        let tmp_path = std::env::temp_dir().join(format!("fframes-{session}"));
        let directory = encoder_options.tmp_files_directory.unwrap_or(&tmp_path);

        if !directory.exists() {
            std::fs::create_dir(directory)?;
        }

        let concurrent_chunks = self.split_video_chunks(ctx.duration_in_frames);

        let files = concurrent_chunks
            .par_iter()
            .enumerate()
            .map(|(thread_number, chunk_range)| {
                let file = directory
                    .join(format!("{thread_number}.{extension}"))
                    .into_os_string()
                    .into_string()
                    .unwrap();

                unsafe {
                    Encoder::with_output(
                        TVideo::WIDTH as i32,
                        TVideo::HEIGHT as i32,
                        TVideo::FPS as i32,
                        file.as_str(),
                        encoder_options,
                        &logger,
                        &mut |encoder| {
                            let mut frame = EncoderFrame::make(&encoder.video_stream)?;

                            let mut svgr_cache = SvgrCache::new(self.cache_capacity);
                            let break_lines_cache = BreaksLruCache::new(self.text_cache_capacity);
                            let mut text_layout_cache =
                                UsvgrTextLayoutCache::new(self.text_cache_capacity);
                            let mut fonts_cache = FontsCache::new();

                            let mut pixmap = svgr::tiny_skia::Pixmap::new(
                                TVideo::WIDTH as u32,
                                TVideo::HEIGHT as u32,
                            )
                            .unwrap();

                            chunk_range
                                .to_owned()
                                .enumerate()
                                .try_for_each(|(index, fr)| {
                                    let svg = video.render_frame(
                                        frame::Frame {
                                            fps: TVideo::FPS,
                                            index: fr,
                                            global_index: fr,
                                            breaks_lru_cache: break_lines_cache.clone(),
                                        },
                                        &ctx,
                                    );

                                    let mut rtree = svg.into_svg_tree(usvg_options).unwrap();
                                    rtree.convert_text_with_cache(
                                        font_db,
                                        &mut text_layout_cache,
                                        &mut fonts_cache,
                                        true,
                                    );

                                    svgr::render(
                                        &rtree,
                                        usvgr::FitTo::Original,
                                        svgr::tiny_skia::Transform::default(),
                                        pixmap.as_mut(),
                                        &mut svgr_cache,
                                    )
                                    .unwrap();
                                    logger.log_frame(index, thread_number);

                                    frame.fill_from_rgba_pixmap(index as i64, pixmap.data());

                                    let video_stream = encoder.video_stream;
                                    encoder.send_frame(&video_stream, &frame)
                                })?;

                            let frames_to_generate = chunk_range.end - chunk_range.start;
                            let submitted_frames =
                                encoder.video_stream.get_frames_in_stream() as usize;

                            if submitted_frames < frames_to_generate {
                                let intra_frames_to_add = frames_to_generate - submitted_frames;

                                for _ in chunk_range.end..chunk_range.end + intra_frames_to_add {
                                    let video_stream = encoder.video_stream;
                                    encoder.send_frame(&video_stream, &frame)?;
                                }
                            }

                            Ok(())
                        },
                    )
                }
                .and_then(std::convert::identity)
                .map_err(|av_err| FFramesError::RenderChunkError(thread_number, av_err))?;

                Ok(file)
            })
            .collect::<FFramesResult<Vec<_>>>()?;

        unsafe {
            concatenator::concat_video_files_with_audio(
                files.as_slice(),
                output,
                timeline.audio_map.as_ref(),
                encoder_options,
                &ctx,
            )?;
        }

        logger.success(output, directory.to_str());
        Ok(())
    }

    fn debug_frame<'a, TVideo: Video + Sync + Sized>(
        &self,
        frame: fframes::Frame,
        out: &str,
        video: TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr_text_layout::fontdb::Database,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()> {
        let mut pixmap = svgr::tiny_skia::Pixmap::new(TVideo::WIDTH as u32, TVideo::HEIGHT as u32)
            .ok_or_else(|| FFramesError::CustomError("Failed to allocate pixmap for rendering. This may indicate that this machine is out of memory.".to_owned()))?;

        let mut rtree = video
            .render_frame(frame, &ctx)
            .into_svg_tree(usvg_options)?;
        rtree.convert_text(font_db, true);

        svgr::render(
            &rtree,
            usvgr::FitTo::Original,
            svgr::tiny_skia::Transform::default(),
            pixmap.as_mut(),
            &mut SvgrCache::none(),
        )
        .ok_or_else(|| FFramesError::CustomError("Failed to render frame".to_owned()))?;

        let buffer = pixmap.encode_png().unwrap();
        std::fs::write(out, buffer)?;

        Ok(())
    }
}
