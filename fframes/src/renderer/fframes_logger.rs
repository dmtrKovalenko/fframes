use super::renderer_error::{FFramesRendererError, FFramesRendererResult};
use crate::media::ffmpeg_sys_fframes::AV_LOG_FATAL;
use colored::*;
use core::fmt::Debug;
use indicatif::ProgressBar;
use once_cell::sync::OnceCell;
use std::{
    ffi::c_int,
    path::{Path, PathBuf},
    sync::Arc,
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

    fn get_libav_log_level(&self) -> c_int {
        AV_LOG_FATAL
    }

    fn should_dump_format_info(&self) -> bool {
        false
    }

    fn success(&self, output_path: &Path, temp_files_dir: Option<&PathBuf>);
}

pub struct CompactFFramesLogger {
    frames_progress_bar: OnceCell<ProgressBar>,
    media_progress_bar: OnceCell<ProgressBar>, }

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
    /// Pass custom logger functionality by implementing FFramesLogger trait
    Custom(Arc<dyn FFramesLogger>),
}

pub fn make_logger(variant: FFramesLoggerVariant) -> Arc<dyn FFramesLogger> {
    match variant {
        FFramesLoggerVariant::Silent => Arc::new(SilentLogger) as Arc<dyn FFramesLogger>,
        FFramesLoggerVariant::Quiet => Arc::new(QuietLogger) as Arc<dyn FFramesLogger>,
        // TODO: Implement custom verbose debugging loggger
        FFramesLoggerVariant::Compact | FFramesLoggerVariant::Debug => {
            Arc::new(CompactFFramesLogger {
                frames_progress_bar: OnceCell::new(),
                media_progress_bar: OnceCell::new(),
            }) as Arc<dyn FFramesLogger>
        }
        FFramesLoggerVariant::Custom(logger) => logger,
    }
}
