use super::renderer_error::FFramesRendererResult;
use crate::{
    DynamicMediaProvider, RawFontData,
    media::{CaptionError, RawMediaFile, Subtitles, VideoMedia},
};
use rayon::prelude::*;
use serde::Serialize;
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::{self},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// An owned snapshot of a media directory used to prepare a media provider.
/// Subtitle JSON is parsed once during import and retained for preparation;
/// other media keeps its bytes or source path as required by its decoder.
pub struct MediaDirectory(Vec<(PathBuf, DirectoryMedia)>);

enum DirectoryMedia {
    Raw(RawMediaFile),
    // Keep subtitle errors until preparation, matching the other media decoders.
    // These become user errors with the file path, as with ordinary subtitle loading.
    Subtitles(Result<Subtitles, String>),
}

/// What the directory importer accepted or skipped, before decoding.
#[derive(Debug, Default, Serialize)]
pub struct MediaImportReport {
    pub accepted: Vec<PathBuf>,
    pub skipped: Vec<SkippedMediaFile>,
}

#[derive(Debug, Serialize)]
pub struct SkippedMediaFile {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Copy)]
pub struct MediaProcessingOptions {
    /// Decode audio from video containers as well as standalone audio files.
    pub decode_video_audio: bool,
}

impl Default for MediaProcessingOptions {
    fn default() -> Self {
        Self {
            decode_video_audio: true,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MediaDirectory {
    pub fn read_folder(folder_path: impl AsRef<Path>) -> FFramesRendererResult<MediaDirectory> {
        let (directory, report) = Self::read_folder_with_report(folder_path)?;
        for skipped in report.skipped {
            eprintln!(
                "fframes: skipped {}: {}",
                skipped.path.display(),
                skipped.reason
            );
        }
        Ok(directory)
    }

    /// Import without printing, returning every accepted and skipped path. File names keep
    /// their original case; only extension matching is case insensitive. I/O errors fail.
    pub fn read_folder_with_report(
        folder_path: impl AsRef<Path>,
    ) -> FFramesRendererResult<(Self, MediaImportReport)> {
        let mut files = fs::read_dir(folder_path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        files.sort();
        // File reads and subtitle document parsing are independent. Keep them
        // outside provider locks and preserve sorted report order on collection.
        let files = files
            .into_par_iter()
            .map(|path| -> io::Result<_> {
                let json = if path.is_file()
                    && path
                        .extension()
                        .and_then(OsStr::to_str)
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                {
                    let bytes = fs::read(&path)?;
                    Some(match parse_subtitle_json_document(&path, &bytes) {
                        Some(subtitles) => DirectoryMedia::Subtitles(subtitles),
                        None => DirectoryMedia::Raw(RawMediaFile::Data(bytes)),
                    })
                } else {
                    None
                };
                Ok((path, json))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let mut folder_content = Vec::new();
        let mut report = MediaImportReport::default();
        for (path, json) in files {
            let extension = path
                .extension()
                .and_then(OsStr::to_str)
                .map(str::to_ascii_lowercase);
            let reason = if path.is_file() {
                match extension.as_deref() {
                    Some("json") if matches!(&json, Some(DirectoryMedia::Subtitles(_))) => None,
                    Some("json") if cfg!(feature = "lottie") => None,
                    Some("json") => Some(
                        "JSON is not recognized as subtitles; animations require the lottie feature",
                    ),
                    Some(
                        "jpg" | "jpeg" | "png" | "vtt" | "mp3" | "wav" | "flac" | "aac" | "pcm"
                        | "ogg" | "mp2" | "m4a" | "mp4" | "webm" | "mkv" | "avi" | "mov" | "flv"
                        | "wmv" | "m4v" | "ttf" | "ttc" | "otf" | "otc",
                    ) => None,
                    _ => Some("unsupported media extension"),
                }
            } else {
                Some("not a file (directory import is not recursive)")
            };
            if let Some(reason) = reason {
                report.skipped.push(SkippedMediaFile {
                    path,
                    reason: reason.to_owned(),
                });
                continue;
            }
            // Audio is decoded from its path below. Reading the compressed file into an
            // unused byte vector first wastes both memory and I/O on every command.
            let media = if let Some(json) = json {
                json
            } else {
                DirectoryMedia::Raw(match extension.as_deref() {
                    Some("jpg" | "jpeg" | "png" | "vtt") => RawMediaFile::Data(fs::read(&path)?),
                    _ => RawMediaFile::Stream(path.clone()),
                })
            };
            report.accepted.push(path.clone());
            folder_content.push((path, media));
        }
        Ok((MediaDirectory(folder_content), report))
    }

    pub fn process_media_source(&self) -> FFramesRendererResult<DynamicMediaProvider<'_>> {
        self.process_media_source_with_options(MediaProcessingOptions::default())
    }

    /// Prepare media, optionally skipping audio embedded in video containers.
    /// Explicit audio files are always decoded; subtitle metadata stays lazy.
    pub fn process_media_source_with_options(
        &self,
        options: MediaProcessingOptions,
    ) -> FFramesRendererResult<DynamicMediaProvider<'_>> {
        let audio_hash = Mutex::new(HashMap::new());
        let subtitles_hash = Mutex::new(HashMap::new());
        let image_hash = Mutex::new(HashMap::new());
        let fontdata = Mutex::new(Vec::new());
        let video_paths = Mutex::new(HashMap::new());
        #[cfg(feature = "lottie")]
        let lottie_hash = Mutex::new(HashMap::new());

        self.0
            .par_iter()
            .try_for_each(|(path, media)| -> FFramesRendererResult<()> {
                if let Some((extension, filename)) = path
                    .extension()
                    .and_then(OsStr::to_str)
                    .zip(path.file_name().and_then(OsStr::to_str))
                {
                    let raw_file = match media {
                        DirectoryMedia::Subtitles(subtitles) => {
                            let subtitles = subtitles
                                .as_ref()
                                .map_err(|error| {
                                    crate::error::FFramesError::UserError(format!(
                                        "{}: {error}",
                                        path.display()
                                    ))
                                })?
                                .clone();
                            subtitles_hash
                                .lock()?
                                .insert(filename.to_owned(), subtitles);
                            return Ok(());
                        }
                        DirectoryMedia::Raw(raw_file) => raw_file,
                    };
                    match (extension.to_ascii_lowercase().as_str(), raw_file) {
                        ("mp3" | "wav" | "flac" | "aac" | "pcm" | "ogg" | "mp2" | "m4a", _) => {
                            let audio_data =
                                crate::media::PreloadedAudioData::decode_raw_file_stereo(
                                    None, path,
                                )?;

                            audio_hash.lock()?.insert(
                                filename.to_owned(),
                                crate::AudioData::Preloaded(audio_data),
                            );
                        }
                        ("vtt", RawMediaFile::Data(bytes)) => {
                            let subtitles = parse_subtitles(path, bytes)?;
                            subtitles_hash
                                .lock()?
                                .insert(filename.to_owned(), subtitles);
                        }
                        #[cfg(feature = "lottie")]
                        ("json", RawMediaFile::Data(bytes)) => {
                            lottie_hash.lock()?.insert(
                                filename.to_owned(),
                                crate::Lottie::parse(bytes)
                                    .map_err(crate::error::FFramesError::from)?,
                            );
                        }
                        ("ttf" | "ttc" | "otf" | "otc", RawMediaFile::Stream(path)) => {
                            let bytes = fs::read(path)?;
                            fontdata.lock()?.push(RawFontData {
                                file_name: filename.to_owned(),
                                data: Arc::new(bytes),
                            });
                        }
                        ("jpg" | "jpeg" | "png", RawMediaFile::Data(bytes)) => {
                            image_hash.lock()?.insert(
                                filename.to_owned(),
                                crate::media::ImageData::new_from_bytes(filename, bytes)?,
                            );
                        }
                        ("mp4" | "webm" | "mkv" | "avi" | "mov" | "flv" | "wmv" | "m4v", _) => {
                            video_paths.lock()?.insert(
                                filename.to_owned(),
                                VideoMedia {
                                    path: path.to_owned(),
                                    metadata: None,
                                },
                            );

                            if options.decode_video_audio
                                && let Ok(audio_data) =
                                    crate::media::PreloadedAudioData::decode_raw_file_stereo(
                                        None, path,
                                    )
                            {
                                audio_hash.lock()?.insert(
                                    filename.to_owned(),
                                    crate::AudioData::Preloaded(audio_data),
                                );
                            }
                        }
                        _ => {}
                    }
                }

                Ok(())
            })?;

        Ok(DynamicMediaProvider {
            audio: audio_hash.into_inner()?,
            images: image_hash.into_inner()?,
            subtitles: subtitles_hash.into_inner()?,
            videos: video_paths.into_inner()?,
            fontdata: fontdata.into_inner()?,
            #[cfg(feature = "lottie")]
            lottie: lottie_hash.into_inner()?,
        })
    }
}

fn parse_subtitle_json_document(path: &Path, bytes: &[u8]) -> Option<Result<Subtitles, String>> {
    let named = path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower == "subtitles.json"
                || lower == "captions.json"
                || lower.ends_with(".subtitles.json")
                || lower.ends_with(".captions.json")
        });
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => return named.then(|| Err(error.to_string())),
    };
    // Explicit subtitle names have always used the content-detecting parser,
    // including a WebVTT document saved with a .json suffix.
    if named
        && text
            .trim_start()
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with("WEBVTT")
    {
        return Some(Subtitles::parse(text).map_err(|error| error.to_string()));
    }
    match Subtitles::parse_json_document(text) {
        Ok(Some(subtitles)) => Some(Ok(subtitles)),
        Ok(None) => named.then(|| {
            Err("expected a JSON object with a cues array for named subtitle media".into())
        }),
        Err(CaptionError::Json(error)) if error.is_syntax() || error.is_eof() => {
            named.then(|| Err(CaptionError::Json(error).to_string()))
        }
        // Shape and timing errors mean a cues array was recognized. Keep these
        // in the subtitle path instead of accidentally treating them as Lottie.
        Err(error) => Some(Err(error.to_string())),
    }
}

fn parse_subtitles(path: &Path, bytes: &[u8]) -> FFramesRendererResult<Subtitles> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        crate::error::FFramesError::UserError(format!("{}: {error}", path.display()))
    })?;
    Subtitles::parse(text).map_err(|error| {
        crate::error::FFramesError::UserError(format!("{}: {error}", path.display())).into()
    })
}
