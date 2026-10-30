//! Real-time native preview of an fframes [`Video`] in a desktop window, rendered by the
//! Skia backend. The GPU backends draw into the swapchain of the window (a `CAMetalLayer`
//! drawable or a Vulkan swapchain image) and no frame is read back.
//!
//! ```rust,ignore
//! fn main() {
//!     let media = MyVideoMedia::prepare().unwrap();
//!     let video = MyVideo { media: &media };
//!
//!     fframes_native_player::play(&video, &fframes_native_player::PlayerOptions {
//!         media: Some(&media),
//!         ..Default::default()
//!     })
//!     .unwrap();
//! }
//! ```
//!
//! Controls (vim keys or arrows):
//! - `Space` (or click the video): play/pause
//! - `h`/`l` or `←`/`→`: seek one second back/forward
//! - `j`/`k` or `↓`/`↑` (also `,`/`.`): step one frame back/forward, pauses
//! - `g`/`G` or `Home`/`End`: first/last frame, `0`-`9`: jump to 0-90%
//! - Studio keeps playback controls above the timeline. Focus mode shows a floating
//!   dock with a seek slider while paused or on pointer activity; `b` hides the dock.
//! - click a scene to jump to its start, or drag across the timeline to scrub
//! - `[` / `]`: previous/next scene start, `t`: switch between the studio and video-only views
//! - scroll the audio lanes to browse overlapping clips
//! - `r`: toggle looping, `f`: full screen, `q`/`Esc`: quit
//!
//! [`play`] must be called from the main thread and only once per process (a winit
//! requirement on macOS).

mod app;
#[cfg(feature = "audio")]
mod audio;
mod controls;
mod error;
mod icons;
mod options;
mod presenter;
mod scheduler;
mod studio;

pub use error::*;
pub use options::*;

use std::collections::HashMap;
use std::sync::Mutex;

use fframes::{FFramesContext, FFramesRendererRuntime, TimeBase, Video, VideoSize, usvgr};
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;

use crate::app::{App, AppConfig, FrameReady};
use crate::scheduler::{Scheduler, WorkerKind};

/// Opens a window and plays `video` in real time until the window is closed.
///
/// Frames that can not be generated in time are dropped, so heavy videos play at a lower
/// frame rate but stay in sync with the clock and the audio.
pub fn play<'a, 'media: 'a, TVideo: Video + Sync>(
    video: &'a TVideo,
    options: &PlayerOptions<'a, 'media>,
) -> PlayerResult<()> {
    let event_loop = EventLoop::<FrameReady>::with_user_event().build()?;

    #[cfg(feature = "audio")]
    let mut audio = None;
    let audio_sample_rate = || {
        #[cfg(feature = "audio")]
        if options.audio {
            audio = crate::audio::AudioOutput::new()
                .inspect_err(|err| eprintln!("fframes player: audio disabled ({err})"))
                .ok();
            if let Some(output) = &audio {
                return output.sample_rate;
            }
        }
        44100
    };
    let scenes = video.define_scenes();
    let FFramesRendererRuntime {
        time_base,
        timeline,
        mut font_source,
    } = FFramesRendererRuntime::with_audio_sample_rate(
        TimeBase {
            fps: TVideo::FPS,
            sample_rate: 44100,
        },
        video,
        &scenes,
        options.media,
        audio_sample_rate,
    )?;

    if options.load_system_fonts {
        font_source.load_system_fonts();
    }

    let mut image_source = HashMap::new();
    if let Some(media) = options.media {
        media.populate_image_source(&mut image_source);
    }

    // The player draws with the Skia renderer, which draws `FastShape`s natively.
    let usvg_options = usvgr::Options {
        image_data: Some(&image_source),
        font_family: options.default_font.to_string(),
        fast_shapes: true,
        ..Default::default()
    };

    let ctx = FFramesContext {
        time_base,
        mode: fframes::FFramesMode::Renderer,
        media_source: options.media,
        duration_in_frames: timeline.duration_in_frames,
        scenes: timeline.scenes.as_ref(),
        font_source: Some(&font_source),
        abort_signal: None,
        current_video_size: VideoSize {
            width: TVideo::WIDTH,
            height: TVideo::HEIGHT,
        },
    };

    // The window owns the Skia context, so the trees of `render_svgr` render on the CPU.
    let svgr_images = fframes::SvgrImages::new(Some(Box::new(fframes::cpu::CpuSvgrRasterizer)));

    let window_size = initial_window_size::<TVideo>(options);
    let aspect = TVideo::WIDTH as f32 / TVideo::HEIGHT.max(1) as f32;
    let tiles = if aspect < 1. {
        window_size.height as f32 / 60.
    } else {
        window_size.width as f32 / (60. * aspect)
    };
    let thumbnail_count = (tiles.ceil() as usize)
        .clamp(1, 32)
        .min(timeline.duration_in_frames);
    let scheduler = Scheduler::new(
        timeline.duration_in_frames,
        options.lookahead(),
        options.looping,
        thumbnail_count,
        options.software_decode,
    );
    let proxy = Mutex::new(event_loop.create_proxy());
    let notify_frame_ready = |event| {
        let _ = proxy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send_event(event);
    };

    let mut app = App::new(
        AppConfig {
            title: options.title,
            window_size,
            fit_portrait_to_screen: aspect < 1. && options.window_size.is_none(),
            backend: options.backend,
            vsync: options.vsync,
            studio: studio::Studio::new(
                time_base,
                &timeline,
                options.media,
                thumbnail_count,
                aspect,
            ),
            fps: TVideo::FPS,
            duration_in_frames: timeline.duration_in_frames,
            background: TVideo::BACKGROUND_COLOR,
            looping: options.looping,
            autoplay: options.autoplay,
            start_frame: options.start_frame,
        },
        &scheduler,
        #[cfg(feature = "audio")]
        audio.as_ref(),
    );

    let run_worker = |kind| {
        let db = font_source.as_db_ref();
        svgr_images.enter(&usvg_options, db, || {
            scheduler.run_worker(video, &ctx, &usvg_options, db, kind, &notify_frame_ready);
        });
    };
    let result = std::thread::scope(|scope| {
        for _ in 0..options.render_threads() {
            scope.spawn(|| run_worker(WorkerKind::Playback));
        }
        scope.spawn(|| run_worker(WorkerKind::Thumbnails));

        #[cfg(feature = "audio")]
        if let (Some(audio), Some(audio_map)) = (audio.as_ref(), timeline.audio_map.as_ref()) {
            scope.spawn(|| audio.run_feeder(&ctx, audio_map, options.audio_mix));
        }

        // The event loop has to run on the main thread (macOS).
        let result = event_loop.run_app(&mut app);

        scheduler.shutdown();
        #[cfg(feature = "audio")]
        if let Some(audio) = audio.as_ref() {
            audio.shutdown();
        }

        result
    });

    if let Some(err) = app.error.take() {
        return Err(err);
    }

    Ok(result?)
}

/// The `preview` command of `fframes::cli`: plays the video with the media, fonts and audio
/// mix of the render options.
///
/// ```rust,ignore
/// fframes::cli::run_with_preview(cli, &video, options, make_backend, &mut frame_renderer,
///     fframes_native_player::cli_preview)
/// ```
pub fn cli_preview<'a, 'media: 'a, TVideo: Video + Sync>(
    video: &'a TVideo,
    options: &fframes::RenderOptions<'a, 'media>,
    request: &fframes::PreviewRequest,
) -> Result<(), String> {
    let backend = match request.backend.as_str() {
        "auto" => PlayerBackend::Auto,
        "cpu" => PlayerBackend::Cpu,
        #[cfg(feature = "metal")]
        "metal" => PlayerBackend::Metal,
        #[cfg(feature = "vulkan")]
        "vulkan" => PlayerBackend::Vulkan,
        other => {
            let mut available = vec!["auto"];
            #[cfg(feature = "metal")]
            available.push("metal");
            #[cfg(feature = "vulkan")]
            available.push("vulkan");
            available.push("cpu");
            return Err(format!(
                "backend \"{other}\" is not available in this build, use one of: {}",
                available.join(", ")
            ));
        }
    };

    let title = std::env::args()
        .next()
        .and_then(|program| {
            std::path::Path::new(&program)
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "fframes".to_owned());

    play(
        video,
        &PlayerOptions {
            media: options.media,
            load_system_fonts: options.load_system_fonts,
            default_font: options.default_font,
            title: &title,
            backend,
            autoplay: request.autoplay,
            looping: request.looping,
            start_frame: request.start_frame,
            audio: request.audio,
            software_decode: request.software_decode,
            audio_mix: options.audio_mix,
            ..Default::default()
        },
    )
    .map_err(|err| err.to_string())
}

fn initial_window_size<TVideo: Video>(options: &PlayerOptions) -> LogicalSize<u32> {
    if let Some((width, height)) = options.window_size {
        return LogicalSize::new(width, height);
    }

    let (width, height) = (TVideo::WIDTH as f64, TVideo::HEIGHT as f64);
    let max_height = if width < height { 960. } else { 720. };
    let scale = (1280. / width).min(max_height / height).min(1.);
    let sidebar = if width < height {
        studio::PORTRAIT_SIDEBAR_WIDTH
    } else {
        0.
    };
    LogicalSize::new(
        (width * scale + f64::from(sidebar)) as u32,
        (height * scale) as u32,
    )
}
