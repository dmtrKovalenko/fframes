use super::renderer_error::{FFramesRendererError, FFramesRendererResult};
use crate::media::ffmpeg_sys_fframes::AV_LOG_FATAL;
use colored::*;
use core::fmt::Debug;
use indicatif::ProgressBar;
use once_cell::sync::OnceCell;
use std::{
    ffi::c_int,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[allow(unused_variables)]
pub trait FFramesLogger: Sync + Send {
    fn init_media_processing(&self, media_count: usize) -> FFramesRendererResult<()> {
        Ok(())
    }
    fn log_processed_media(&self, path: &Path) {}
    fn log_unprocessed_media_file(&self, filename: &str) {}
    fn log_media_processing_start(&self, filename: &str, path: &Path) {}

    fn init_frames_rendering(&self, duration_in_frames: usize) -> FFramesRendererResult<()> {
        Ok(())
    }
    fn log_frame(&self, index: usize, thread_number: usize) {}

    /// Called once before audio encoding starts with the number of audio
    /// frames that will be reported through `log_audio_frame`.
    fn init_audio_encoding(&self, frames_count: usize) -> FFramesRendererResult<()> {
        Ok(())
    }
    fn log_audio_frame(&self) {}
    fn finish_audio_encoding(&self) {}

    fn get_libav_log_level(&self) -> c_int {
        AV_LOG_FATAL
    }

    fn should_dump_format_info(&self) -> bool {
        false
    }

    fn success(&self, output_path: &Path, temp_files_dir: Option<&PathBuf>);

    /// A problem that did not stop the render, e.g. media that `render_frame` asked for
    /// but the media provider does not have.
    fn warn(&self, message: &str) {
        eprintln!("{} {message}", "warning:".yellow().bold());
    }
}

pub struct CompactFFramesLogger {
    frames_progress_bar: OnceCell<ProgressBar>,
    media_progress_bar: OnceCell<ProgressBar>,
    // Reset on every render so a logger instance can be reused.
    audio_progress_bar: Mutex<Option<ProgressBar>>,
}

impl FFramesLogger for CompactFFramesLogger {
    fn init_frames_rendering(&self, frames_count: usize) -> FFramesRendererResult<()> {
        self.frames_progress_bar
            .set(ProgressBar::new(frames_count as u64))
            .map_err(|_| FFramesRendererError::ConcurrencyError)?;

        println!("\nRendering {} frames", frames_count.to_string().cyan());
        Ok(())
    }

    fn success(&self, output_path: &Path, temp_files_dir: Option<&PathBuf>) {
        println!(
            "{} Watch your video: \n$ {} {}\n\n{}",
            "Success!".green().bold(),
            "ffplay".bold(),
            output_path.display(),
            temp_files_dir
                .map(|path| format!("Generated files {}\n", path.display()))
                .unwrap_or_default()
        );
    }

    fn log_frame(&self, _index: usize, _thread_number: usize) {
        if let Some(pb) = self.frames_progress_bar.get() {
            pb.inc(1)
        }
    }

    fn log_unprocessed_media_file(&self, filename: &str) {
        println!("Can not process media file {filename}.")
    }

    fn init_audio_encoding(&self, frames_count: usize) -> FFramesRendererResult<()> {
        println!("Encoding audio stream");
        let mut slot = self
            .audio_progress_bar
            .lock()
            .map_err(|_| FFramesRendererError::ConcurrencyError)?;
        if let Some(previous) = slot.take() {
            previous.finish_and_clear();
        }
        *slot = Some(ProgressBar::new(frames_count as u64));
        Ok(())
    }

    fn log_audio_frame(&self) {
        if let Ok(slot) = self.audio_progress_bar.lock()
            && let Some(pb) = slot.as_ref()
        {
            pb.inc(1);
        }
    }

    fn finish_audio_encoding(&self) {
        if let Ok(mut slot) = self.audio_progress_bar.lock()
            && let Some(pb) = slot.take()
        {
            pb.finish_and_clear();
        }
    }

    fn init_media_processing(&self, medias_count: usize) -> FFramesRendererResult<()> {
        println!(
            "Processing {medias_count} media files",
            medias_count = medias_count.to_string().cyan()
        );

        let progress_bar = ProgressBar::new(medias_count as u64);
        self.media_progress_bar
            .set(progress_bar)
            .map_err(|_| FFramesRendererError::ConcurrencyError)?;

        Ok(())
    }

    fn log_processed_media(&self, _path: &Path) {
        let pb = self
            .media_progress_bar
            .get()
            .expect("Media progress bar not initialized");

        pb.inc(1);
    }
}

pub struct SilentLogger;

impl FFramesLogger for SilentLogger {
    fn success(&self, _output_path: &Path, _temp_files_dir: Option<&PathBuf>) {}
    fn warn(&self, _message: &str) {}
}

pub struct QuietLogger;
impl FFramesLogger for QuietLogger {
    fn success(&self, output_path: &Path, _temp_files_dir: Option<&PathBuf>) {
        println!("Success. Your video {}", output_path.display());
    }
}

impl Debug for dyn FFramesLogger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Logger")
    }
}

#[derive(Default, Debug, Clone)]
/// Different options for logging rendering process.
pub enum FFramesLoggerVariant {
    /// No output
    Silent,
    /// Doesn't show progress of rendering, only the output. Slightly faster.
    Quiet,
    /// Renders one progress bar showing rendering progress frame by frame
    #[default]
    Compact,
    /// Verbose logging for debugging purpose
    Debug,
    /// One plain line per 10% of progress on stderr, no progress bars or colors. Readable in
    /// CI logs and by agents that capture the output.
    Lines,
    /// One JSON object per line on stderr (`{"event":"progress","done":10,"total":100}`).
    Json,
    /// Pass custom logger functionality by implementing FFramesLogger trait
    Custom(Arc<dyn FFramesLogger>),
}

pub fn make_logger(variant: FFramesLoggerVariant) -> Arc<dyn FFramesLogger> {
    match variant {
        FFramesLoggerVariant::Silent => Arc::new(SilentLogger) as Arc<dyn FFramesLogger>,
        FFramesLoggerVariant::Quiet => Arc::new(QuietLogger) as Arc<dyn FFramesLogger>,
        // TODO: Implement custom verbose debugging logger
        FFramesLoggerVariant::Compact | FFramesLoggerVariant::Debug => {
            Arc::new(CompactFFramesLogger {
                frames_progress_bar: OnceCell::new(),
                media_progress_bar: OnceCell::new(),
                audio_progress_bar: Mutex::new(None),
            }) as Arc<dyn FFramesLogger>
        }
        FFramesLoggerVariant::Lines => Arc::new(LinesLogger::new(false)),
        FFramesLoggerVariant::Json => Arc::new(LinesLogger::new(true)),
        FFramesLoggerVariant::Custom(logger) => logger,
    }
}

/// Backs `FFramesLoggerVariant::Lines` and `FFramesLoggerVariant::Json`.
pub struct LinesLogger {
    json: bool,
    started: std::time::Instant,
    total: AtomicUsize,
    done: AtomicUsize,
    reported_decile: AtomicUsize,
}

impl LinesLogger {
    pub fn new(json: bool) -> Self {
        Self {
            json,
            started: std::time::Instant::now(),
            total: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
            reported_decile: AtomicUsize::new(0),
        }
    }

    fn emit(&self, event: &str, text: String, fields: &[(&str, String)]) {
        if self.json {
            let mut line = format!("{{\"event\":{}", json_string(event));
            for (key, value) in fields {
                line.push_str(&format!(",{}:{value}", json_string(key)));
            }
            line.push('}');
            eprintln!("{line}");
        } else {
            eprintln!("{text}");
        }
    }
}

/// Minimal JSON string escaping, the logger must not depend on a serializer.
pub(crate) fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for c in value.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            c if (c as u32) < 0x20 => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

impl FFramesLogger for LinesLogger {
    fn init_frames_rendering(&self, frames_count: usize) -> FFramesRendererResult<()> {
        self.total.store(frames_count, Ordering::Relaxed);
        self.done.store(0, Ordering::Relaxed);
        self.reported_decile.store(0, Ordering::Relaxed);
        self.emit(
            "start",
            format!("rendering {frames_count} frames"),
            &[("frames", frames_count.to_string())],
        );
        Ok(())
    }

    fn log_frame(&self, _index: usize, _thread_number: usize) {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        let total = self.total.load(Ordering::Relaxed).max(1);
        let decile = done * 10 / total;
        if decile > self.reported_decile.fetch_max(decile, Ordering::Relaxed) {
            let elapsed = self.started.elapsed().as_secs_f32();
            self.emit(
                "progress",
                format!("frames {done}/{total} ({}%) {elapsed:.1}s", decile * 10),
                &[
                    ("done", done.to_string()),
                    ("total", total.to_string()),
                    ("elapsed", format!("{elapsed:.3}")),
                ],
            );
        }
    }

    fn init_audio_encoding(&self, frames_count: usize) -> FFramesRendererResult<()> {
        self.emit(
            "audio",
            "encoding audio".to_owned(),
            &[("frames", frames_count.to_string())],
        );
        Ok(())
    }

    fn log_unprocessed_media_file(&self, filename: &str) {
        self.warn(&format!("can not process media file {filename}"));
    }

    fn init_media_processing(&self, media_count: usize) -> FFramesRendererResult<()> {
        self.emit(
            "media",
            format!("processing {media_count} media files"),
            &[("files", media_count.to_string())],
        );
        Ok(())
    }

    fn warn(&self, message: &str) {
        self.emit(
            "warning",
            format!("warning: {message}"),
            &[("message", json_string(message))],
        );
    }

    fn success(&self, output_path: &Path, _temp_files_dir: Option<&PathBuf>) {
        let elapsed = self.started.elapsed().as_secs_f32();
        let output = output_path.display().to_string();
        self.emit(
            "done",
            format!("wrote {output} in {elapsed:.1}s"),
            &[
                ("output", json_string(&output)),
                ("elapsed", format!("{elapsed:.3}")),
            ],
        );
    }
}
