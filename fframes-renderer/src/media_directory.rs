use crate::renderer_error::FFramesRendererResult;
use fframes::{
    media::{decode_image, Subtitles},
    DynamicMediaProvider, RawFontData,
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
pub struct MediaDirectory(Vec<(PathBuf, Vec<u8>)>);

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
            if path.is_file() {
                let content = std::fs::read(&path)?;
                folder_content.push((path, content))
            }
        }

        Ok(MediaDirectory(folder_content))
    }

    pub fn process_media_source(
        &self,
        // logger: &Arc<dyn FFramesLogger>,
    ) -> FFramesRendererResult<DynamicMediaProvider> {
        let audio_hash = Mutex::new(HashMap::new());
        let subtitles_hash = Mutex::new(HashMap::new());
        let image_hash = Mutex::new(HashMap::new());
        let fontdata = Mutex::new(Vec::new());

        // logger.init_media_processing(self.0.len())?;

        self.0
            .par_iter()
            .try_for_each(|(path, bytes)| -> FFramesRendererResult<()> {
                if let Some((extension, filename)) = path
                    .extension()
                    .and_then(OsStr::to_str)
                    .zip(path.file_name().and_then(OsStr::to_str))
                {
                    // logger.log_media_processing_start(filename, &path);
                    match extension {
                        "mp3" | "wav" | "flac" | "aac" | "pcm" | "ogg" | "mp2" => {
                            // todo pull out to the options
                            const SAMPLE_RATE: u32 = 44100;

                            let audio_data = fframes::media::PreloadedAudioData::decode_buffer(
                                Some(SAMPLE_RATE),
                                &path.to_string_lossy(),
                                bytes,
                            )?;

                            audio_hash.lock()?.insert(
                                filename.to_owned(),
                                fframes::AudioData::Preloaded(audio_data),
                            );
                        }
                        "vtt" => {
                            let str_bytes = std::str::from_utf8(bytes)?;

                            subtitles_hash.lock()?.insert(
                                filename.to_owned(),
                                Subtitles::parse(str_bytes)
                                    .map_err(fframes::media::FFramesMediaError::from)?,
                            );
                        }
                        "ttf" | "ttc" | "otf" | "otc" => {
                            fontdata.lock()?.push(RawFontData {
                                file_name: filename.to_owned(),
                                data: Arc::new(bytes.to_owned()),
                            });
                        }
                        "jpg" | "jpeg" | "png" => {
                            let image = decode_image(filename, bytes)
                                .map_err(fframes::media::FFramesMediaError::from)?;

                            image_hash.lock()?.insert(
                                filename.to_owned(),
                                fframes::media::ImageData {
                                    image: Arc::new(image),
                                    filename: filename.to_owned(),
                                },
                            );
                        }
                        "DS_Store" => (),
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
            fontdata.into_inner()?,
        );

        Ok(provider)
    }
}
