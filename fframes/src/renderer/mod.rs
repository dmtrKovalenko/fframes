#![allow(clippy::module_inception)]
#![allow(clippy::missing_safety_doc)]

#[cfg(feature = "cpu_renderer")]
pub mod cpu;
#[doc(hidden)]
pub mod pix_fmt;

pub mod concatenator;

mod encoder_frame;
mod ffmpeg_helper;
mod frame_export;
pub use frame_export::*;
mod renderer_font_source;
mod stream;

mod encoder;
pub use encoder::*;

pub mod fframes_logger;
pub use fframes_logger::*;

mod media_directory;
pub use media_directory::*;

mod render_backend;
pub use render_backend::*;

mod renderer;
pub use renderer::*;

mod renderer_error;
pub use renderer_error::*;

mod scheduler;
pub use scheduler::*;

/// The scheduler of a render: `frame_range.len()` frames over `workers`, with segment
/// boundaries that prefer the frame after a scene ends (so a video clip of the next scene
/// is decoded from its start, or from the overlap at most) and, when the render decodes
/// video files, no tail helping below [`MIN_HELP_FRAMES_WITH_VIDEO`] frames.
#[doc(hidden)]
pub fn frame_scheduler(
    ctx: &crate::FFramesContext<'_, '_>,
    render_options: &RenderOptions<'_, '_>,
    frame_range: &std::ops::Range<usize>,
    workers: usize,
) -> FrameScheduler {
    let cut_points: Vec<usize> = ctx
        .scenes
        .map(crate::ResolvedScenesTimeline::scene_ends)
        .unwrap_or_default()
        .into_iter()
        .filter(|end| frame_range.contains(end))
        .map(|end| end - frame_range.start)
        .collect();
    let decodes_video = render_options
        .media
        .is_some_and(|media| !media.get_all_video_data().is_empty());
    FrameScheduler::with_cut_points(
        frame_range.len(),
        workers,
        render_options
            .video_encoder_options
            .min_segment_frames(ctx.time_base.fps),
        &cut_points,
        if decodes_video {
            MIN_HELP_FRAMES_WITH_VIDEO
        } else {
            1
        },
    )
}

mod segment_writer;
pub use segment_writer::*;
mod frame_guard;
pub use frame_guard::*;

mod preview;
pub use preview::*;

#[cfg(feature = "cpu_renderer")]
pub mod sheet;
pub mod snapshot;

#[cfg(feature = "cli")]
pub mod cli;

pub use rayon;
#[cfg(test)]
mod tests;
