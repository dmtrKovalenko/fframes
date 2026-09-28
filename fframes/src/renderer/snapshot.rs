//! Visual regression of individual frames: render frames addressed by time specs, compare
//! them with approved PNGs and write the actual frame and a diff image when they differ.
//!
//! ```ignore
//! #[test]
//! fn intro_looks_right() {
//!     let video = MyVideo::new();
//!     let mut previewer = fframes::Previewer::new(&video, &options).unwrap();
//!     fframes::snapshot::assert_frames(
//!         &mut previewer,
//!         &mut fframes::CpuFrameRenderer::default(),
//!         &["Intro@0.5s", "Intro@end", "50%"],
//!         &fframes::snapshot::SnapshotOptions::default(),
//!     );
//! }
//! ```
//!
//! Missing snapshots are written on the first run. Set `FFRAMES_UPDATE_SNAPSHOTS=1` to accept
//! changed frames.
use super::{FFramesRendererError, FFramesRendererResult, FrameRenderer, Previewer, RgbaFrame};
use crate::Video;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SnapshotOptions {
    pub directory: PathBuf,
    /// A pixel differs when any channel differs by more than this (0-255). Absorbs
    /// anti-aliasing differences between machines.
    pub channel_threshold: u8,
    /// The snapshot fails when more than this fraction of pixels differ.
    pub max_diff_ratio: f64,
    /// Overwrite snapshots that differ. Defaults to `FFRAMES_UPDATE_SNAPSHOTS` being set.
    pub update: bool,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("_frame_snapshots"),
            channel_threshold: 16,
            max_diff_ratio: 0.001,
            update: std::env::var("FFRAMES_UPDATE_SNAPSHOTS")
                .is_ok_and(|v| v != "0" && !v.is_empty()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStatus {
    Matched,
    Created,
    Updated,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotResult {
    pub spec: String,
    pub frame: usize,
    pub status: SnapshotStatus,
    pub snapshot: PathBuf,
    /// Fraction of differing pixels (0 when created).
    pub diff_ratio: f64,
    /// Written when the frame differs.
    pub actual: Option<PathBuf>,
    pub diff: Option<PathBuf>,
}

/// `Intro@1.5s` -> `intro_1.5s`
pub fn snapshot_name(spec: &str) -> String {
    let name: String = spec
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' => c.to_ascii_lowercase(),
            '%' => 'p',
            _ => '_',
        })
        .collect();
    name.trim_matches('_').to_owned()
}

/// Compares two frames. Returns the fraction of differing pixels and a diff image (the
/// expected frame dimmed, differing pixels red), or `None` for the image when sizes differ.
pub fn diff_frames(
    expected: &RgbaFrame,
    actual: &RgbaFrame,
    channel_threshold: u8,
) -> (f64, Option<RgbaFrame>) {
    if expected.width != actual.width || expected.height != actual.height {
        return (1., None);
    }

    let mut differing = 0usize;
    let mut diff = Vec::with_capacity(expected.pixels.len());
    for (e, a) in expected
        .pixels
        .chunks_exact(4)
        .zip(actual.pixels.chunks_exact(4))
    {
        let differs = e
            .iter()
            .zip(a)
            .any(|(e, a)| e.abs_diff(*a) > channel_threshold);
        if differs {
            differing += 1;
            diff.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            let gray = ((e[0] as u32 + e[1] as u32 + e[2] as u32) / 3 / 4) as u8;
            diff.extend_from_slice(&[gray, gray, gray, 255]);
        }
    }

    let total = (expected.width * expected.height).max(1) as f64;
    (
        differing as f64 / total,
        Some(RgbaFrame {
            width: expected.width,
            height: expected.height,
            pixels: diff,
        }),
    )
}

fn load_png(path: &Path) -> FFramesRendererResult<RgbaFrame> {
    let image = image::open(path)
        .map_err(|err| FFramesRendererError::ImageError((path.display().to_string(), err)))?
        .into_rgba8();
    Ok(RgbaFrame {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    })
}

/// Renders the frame at `spec` and compares it with its snapshot.
pub fn check_frame<'a, 'media: 'a, TVideo: Video>(
    previewer: &mut Previewer<'a, 'media, TVideo>,
    renderer: &mut dyn FrameRenderer,
    spec: &str,
    options: &SnapshotOptions,
) -> FFramesRendererResult<SnapshotResult> {
    let frame = previewer
        .timeline()
        .resolve_frame(spec)
        .map_err(|err| FFramesRendererError::Custom(err.to_string()))?;
    let actual = previewer.render(frame, renderer)?;

    std::fs::create_dir_all(&options.directory)?;
    let name = snapshot_name(spec);
    let snapshot = options.directory.join(format!("{name}.png"));
    let actual_path = options.directory.join(format!("{name}.actual.png"));
    let diff_path = options.directory.join(format!("{name}.diff.png"));
    let _ = std::fs::remove_file(&actual_path);
    let _ = std::fs::remove_file(&diff_path);

    let mut result = SnapshotResult {
        spec: spec.to_owned(),
        frame,
        status: SnapshotStatus::Matched,
        snapshot: snapshot.clone(),
        diff_ratio: 0.,
        actual: None,
        diff: None,
    };

    if !snapshot.exists() {
        actual.save_png(&snapshot)?;
        result.status = SnapshotStatus::Created;
        return Ok(result);
    }

    let expected = load_png(&snapshot)?;
    let (ratio, diff) = diff_frames(&expected, &actual, options.channel_threshold);
    result.diff_ratio = ratio;

    if ratio <= options.max_diff_ratio {
        return Ok(result);
    }

    if options.update {
        actual.save_png(&snapshot)?;
        result.status = SnapshotStatus::Updated;
        return Ok(result);
    }

    actual.save_png(&actual_path)?;
    result.actual = Some(actual_path);
    if let Some(diff) = diff {
        diff.save_png(&diff_path)?;
        result.diff = Some(diff_path);
    }
    result.status = SnapshotStatus::Failed;
    Ok(result)
}

/// Checks every spec and panics with a readable report if any frame differs.
pub fn assert_frames<'a, 'media: 'a, TVideo: Video>(
    previewer: &mut Previewer<'a, 'media, TVideo>,
    renderer: &mut dyn FrameRenderer,
    specs: &[&str],
    options: &SnapshotOptions,
) -> Vec<SnapshotResult> {
    let results: Vec<SnapshotResult> = specs
        .iter()
        .map(|spec| {
            check_frame(previewer, renderer, spec, options)
                .unwrap_or_else(|err| panic!("snapshot {spec}: {err}"))
        })
        .collect();

    let failed: Vec<String> = results
        .iter()
        .filter(|r| r.status == SnapshotStatus::Failed)
        .map(|r| {
            format!(
                "  {} (frame {}): {:.3}% of pixels differ, see {} and {}",
                r.spec,
                r.frame,
                r.diff_ratio * 100.,
                r.actual.as_deref().unwrap_or(Path::new("-")).display(),
                r.diff.as_deref().unwrap_or(Path::new("-")).display(),
            )
        })
        .collect();

    if !failed.is_empty() {
        panic!(
            "{} frame snapshot(s) differ (FFRAMES_UPDATE_SNAPSHOTS=1 accepts them):\n{}",
            failed.len(),
            failed.join("\n")
        );
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(color: [u8; 4]) -> RgbaFrame {
        RgbaFrame {
            width: 4,
            height: 4,
            pixels: color.repeat(16),
        }
    }

    #[test]
    fn diff_ignores_small_differences() {
        let (ratio, _) = diff_frames(
            &solid([100, 100, 100, 255]),
            &solid([110, 100, 100, 255]),
            16,
        );
        assert_eq!(ratio, 0.);
        let (ratio, diff) = diff_frames(
            &solid([100, 100, 100, 255]),
            &solid([200, 100, 100, 255]),
            16,
        );
        assert_eq!(ratio, 1.);
        assert_eq!(&diff.unwrap().pixels[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn names_are_file_safe() {
        assert_eq!(snapshot_name("Intro@1.5s"), "intro_1.5s");
        assert_eq!(snapshot_name("50%"), "50p");
        assert_eq!(snapshot_name("#3"), "3");
    }
}
