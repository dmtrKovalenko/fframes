use fframes::{frame, video::Video, BreaksLruCache, Duration, ResolvedAudioMap};
use rayon::prelude::*;
use std::{
    ops::{Add, Range},
    sync::Arc,
    time::Instant,
};
use svgr::SvgrCache;
use usvgr_text_layout::{FontsCache, TreeTextToPath, UsvgrTextLayoutCache};
use uuid::Uuid;

use crate::{
    concatenator,
    encoder::{Encoder, EncoderFrame, EncoderOptions},
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
        duration_in_frames: usize,
        encoder_options: EncoderOptions<'a>,
        font_db: &usvgr_text_layout::fontdb::Database,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()>
    where
        Self: Sized;
}

#[derive(Debug)]
pub enum RenderBackendVariant {
    Gpu,
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

impl Default for RenderBackendVariant {
    fn default() -> Self {
        RenderBackendVariant::Cpu
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
        duration_in_frames: usize,
        _encoder_options: EncoderOptions<'a>,
        font_db: &usvgr_text_layout::fontdb::Database,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()> {
        let session = Uuid::new_v4();
        let directory = std::env::temp_dir().join(format!("fframes-{session}"));
        // let directory = std::path::Path::new("test_render");

        if !directory.exists() {
            std::fs::create_dir(&directory)?;
        }

        let concurrent_chunks = self.split_video_chunks(duration_in_frames);
        let resolved_audio_map: Option<ResolvedAudioMap> = video.audio().resolve(&ctx);

        let files = concurrent_chunks
            .par_iter()
            .enumerate()
            .map(|(thread_number, chunk_range)| {
                let file = directory
                    .join(format!("{thread_number}.mp4"))
                    .into_os_string()
                    .into_string()
                    .unwrap();

                unsafe {
                    Encoder::with_output(
                        TVideo::WIDTH as i32,
                        TVideo::HEIGHT as i32,
                        TVideo::FPS as i32,
                        file.as_str(),
                        "libx264",
                        &logger,
                        false,
                        &mut |encoder| {
                            let mut last_svg = "".to_owned();
                            let mut frame = EncoderFrame::make(&encoder.video_stream);

                            let mut svgr_cache = SvgrCache::new(self.cache_capacity);
                            let break_lines_cache = BreaksLruCache::new(self.text_cache_capacity);
                            let mut text_layout_cache =
                                UsvgrTextLayoutCache::new(self.text_cache_capacity);
                            let mut font_cache = FontsCache::new();

                            let mut pixmap = svgr::tiny_skia::Pixmap::new(
                                TVideo::WIDTH as u32,
                                TVideo::HEIGHT as u32,
                            )
                            .unwrap();

                            let mut duration = std::time::Duration::default();
                            chunk_range
                                .to_owned()
                                .enumerate()
                                .try_for_each(|(index, fr)| {
                                    let svg = video
                                        .render_frame(
                                            frame::Frame {
                                                fps: TVideo::FPS,
                                                index: fr,
                                                global_index: fr,
                                                breaks_lru_cache: break_lines_cache.clone(),
                                            },
                                            &ctx,
                                        )
                                        .into_string();

                                    logger.log_frame(index, thread_number, &svg);
                                    if svg != last_svg {
                                        let start = Instant::now();

                                        let mut rtree =
                                            usvgr::Tree::from_str(&svg, usvg_options).unwrap();
                                        duration = duration.add(start.elapsed());

                                        rtree.convert_text_with_cache(
                                            font_db,
                                            &mut text_layout_cache,
                                            &mut font_cache,
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

                                        last_svg = svg;
                                    }

                                    frame.fill_from_rgba_pixmap(index as i64, pixmap.data());

                                    let video_stream = encoder.video_stream;
                                    encoder.send_frame(&video_stream, frame)
                                })?;

                            println!("Time elapsed for string parsing {:?}", duration);

                            let frames_to_generate = chunk_range.end - chunk_range.start;
                            let submitted_frames =
                                encoder.video_stream.get_frames_in_stream() as usize;

                            if submitted_frames < frames_to_generate {
                                let intra_frames_to_add = frames_to_generate - submitted_frames;

                                for _ in chunk_range.end..chunk_range.end + intra_frames_to_add {
                                    let video_stream = encoder.video_stream;
                                    encoder.send_frame(&video_stream, frame)?;
                                }
                            }

                            frame.free();
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
                resolved_audio_map.as_ref(),
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
        let svg = video.render_frame(frame, &ctx).into_string();

        let mut pixmap =
            svgr::tiny_skia::Pixmap::new(TVideo::WIDTH as u32, TVideo::HEIGHT as u32).unwrap();
        let mut rtree = usvgr::Tree::from_str(&svg, usvg_options).unwrap();
        rtree.convert_text(font_db, true);

        svgr::render(
            &rtree,
            usvgr::FitTo::Original,
            svgr::tiny_skia::Transform::default(),
            pixmap.as_mut(),
            &mut SvgrCache::none(),
        )
        .unwrap();

        let buffer = pixmap.encode_png().unwrap();
        std::fs::write(out, buffer)?;

        Ok(())
    }
}
