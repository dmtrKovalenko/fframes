use super::renderer_error::FFramesRendererResult;
use crate::{
    DynamicMediaProvider, RawFontData,
    media::{RawMediaFile, Subtitles, VideoMedia},
};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::{self},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// A struct representing owned media directory which can be used to process and fill
/// the media provider. Basically represented as the vector of bytes where every media file is
/// owned bytes vector.
///
/// Video trait implementation provides direct access to the bytes owned by this struct.
pub struct MediaDirectory(Vec<(PathBuf, RawMediaFile)>);

#[cfg(not(target_arch = "wasm32"))]
impl MediaDirectory {
    pub fn read_folder(folder_path: impl AsRef<Path>) -> FFramesRendererResult<MediaDirectory> {
        let mut folder_content = vec![];

        if !folder_path.as_ref().is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput, // todo change to NotADirectory when this https://github.com/rust-lang/rust/issues/86442 will be stable
                "resources_dir must be a folder",
            )
            .into());
        }

        for file in fs::read_dir(folder_path)? {
            let path = file?.path();
            if !path.is_file() {
                continue;
            }

            match path.extension().and_then(OsStr::to_str) {
                Some(
                    "jpg" | "jpeg" | "png" | "gif" | "vtt" | "mp3" | "wav" | "flac" | "aac" | "pcm"
                    | "ogg" | "mp2",
                ) => {
                    let bytes = fs::read(&path)?;
                    folder_content.push((path, RawMediaFile::Data(bytes)));
                }
                Some(
                    "mp4" | "webm" | "mkv" | "avi" | "mov" | "flv" | "wmv" | "m4v" | "ttf" | "ttc"
                    | "otf" | "otc",
                ) => {
                    folder_content.push((path.clone(), RawMediaFile::Stream(path)));
                }
                _ => {}
            }
        }

        Ok(MediaDirectory(folder_content))
    }

    pub fn process_media_source(
        &self,
        // logger: &Arc<dyn FFramesLogger>,
    ) -> FFramesRendererResult<DynamicMediaProvider> {
        // todo pull out to the options
        const SAMPLE_RATE: u32 = 44100;

        let audio_hash = Mutex::new(HashMap::new());
        let subtitles_hash = Mutex::new(HashMap::new());
        let image_hash = Mutex::new(HashMap::new());
        let fontdata = Mutex::new(Vec::new());
        let video_paths = Mutex::new(HashMap::new());

        // logger.init_media_processing(self.0.len())?;

        self.0
            .par_iter()
            .try_for_each(|(path, raw_file)| -> FFramesRendererResult<()> {
                if let Some((extension, filename)) = path
                    .extension()
                    .and_then(OsStr::to_str)
                    .zip(path.file_name().and_then(OsStr::to_str))
                {
                    // logger.log_media_processing_start(filename, &path);
                    match (extension, raw_file) {
                        ("mp3" | "wav" | "flac" | "aac" | "pcm" | "ogg" | "mp2", _) => {
                            let audio_data = crate::media::PreloadedAudioData::decode_raw_file(
                                Some(SAMPLE_RATE),
                                path,
                            )?;

                            audio_hash.lock()?.insert(
                                filename.to_owned(),
                                crate::AudioData::Preloaded(audio_data),
                            );
                        }
                        ("vtt", RawMediaFile::Data(bytes)) => {
                            let str_bytes = std::str::from_utf8(bytes)?;

                            subtitles_hash.lock()?.insert(
                                filename.to_owned(),
                                Subtitles::parse(str_bytes)
                                    .map_err(crate::media::FFramesMediaError::from)?,
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

                            if let Ok(audio_data) =
                                crate::media::PreloadedAudioData::decode_raw_file(
                                    Some(SAMPLE_RATE),
                                    path,
                                )
                            {
                                audio_hash.lock()?.insert(
                                    filename.to_owned(),
                                    crate::AudioData::Preloaded(audio_data),
                                );
                            }
                        }
                        ("DS_Store", _) => (),
                        _ => {
                            // logger.log_unprocessed_media_file(filename);
                        }
                    };
                };

                // logger.log_processed_media(&path);
                Ok(())
            })?;

        let provider = DynamicMediaProvider::new(
            audio_hash.into_inner()?,
            image_hash.into_inner()?,
            subtitles_hash.into_inner()?,
            video_paths.into_inner()?,
            fontdata.into_inner()?,
        );

        Ok(provider)
    }
}
