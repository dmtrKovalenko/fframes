use colored::*;
use core::fmt::Debug;
use indicatif::ProgressBar;
use once_cell::sync::OnceCell;
use std::{path::Path, sync::Arc};

pub trait FFramesLogger: Sync + Send {
    fn init_media_processing(&self, media_count: usize);
    fn log_processed_media(&self, path: &Path);
    fn log_unprocessed_media_file(&self, filename: &str);
    fn log_media_processing_start(&self, filename: &str, path: &Path);

    fn init_frames_rendering(&self, duration_in_frames: usize);
    fn log_frame(&self, index: usize, thread_number: usize, svg: &str);

    fn success(&self, output_path: &str, temp_files_dir: Option<&str>);
}

pub struct CompactFFramesLogger {
    frames_progress_bar: OnceCell<ProgressBar>,
    media_progress_bar: OnceCell<ProgressBar>,
}

impl FFramesLogger for CompactFFramesLogger {
    fn init_frames_rendering(&self, frames_count: usize) {
        self.frames_progress_bar
            .set(ProgressBar::new(frames_count as u64))
            .unwrap();

        println!("\nRendering {} frames", frames_count.to_string().cyan());
    }

    fn success(&self, output_path: &str, temp_files_dir: Option<&str>) {
        println!(
            "{} Watch your video: \n$ {ffplay} {output_path}\n\n{temp_files_slug}",
            "Success!".green().bold(),
            ffplay = "ffplay".bold(),
            temp_files_slug = temp_files_dir
                .map(|path| format!("Generated files {path}\n"))
                .unwrap_or_default()
        );
    }

    fn log_frame(&self, _index: usize, _thread_number: usize, _svg: &str) {
        self.frames_progress_bar.get().unwrap().inc(1);
    }

    fn log_unprocessed_media_file(&self, filename: &str) {
        println!("Can not process media file {filename}.")
    }

    fn init_media_processing(&self, medias_count: usize) {
        println!(
            "Processing {medias_count} media files",
            medias_count = medias_count.to_string().cyan()
        );

        let progress_bar = ProgressBar::new(medias_count as u64);
        self.media_progress_bar.set(progress_bar).unwrap();
    }

    fn log_media_processing_start(&self, _filename: &str, _path: &Path) {}

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
    fn log_unprocessed_media_file(&self, _filename: &str) {}

    fn init_frames_rendering(&self, _all_frames: usize) {}

    fn log_frame(&self, _index: usize, _thread_number: usize, _svg: &str) {}

    fn success(&self, output_path: &str, _temp_files_dir: Option<&str>) {
        println!("Success. Your video {output_path}");
    }

    fn init_media_processing(&self, _all_frames: usize) {}

    fn log_processed_media(&self, _path: &Path) {}

    fn log_media_processing_start(&self, _filename: &str, _path: &Path) {}
}

impl Debug for dyn FFramesLogger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Logger")
    }
}

#[derive(Debug, Clone)]
/// Different options for logging rendering process.
pub enum FFramesLoggerVariant {
    /// Doesn't show progress of rendering, only the output. Slightly faster.
    Silent,
    /// Renders one progress bar showing rendering progress frame by frame
    Compact,
    /// Verbose logging for debugging purpose
    Debug,
    /// Pass custom logger functionality by implementing FFramesLogger trait
    Custom(Arc<dyn FFramesLogger>),
}

impl Default for FFramesLoggerVariant {
    fn default() -> Self {
        Self::Compact
    }
}

pub fn make_logger(variant: FFramesLoggerVariant) -> Arc<dyn FFramesLogger> {
    match variant {
        FFramesLoggerVariant::Silent => Arc::new(SilentLogger) as Arc<dyn FFramesLogger>,
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
