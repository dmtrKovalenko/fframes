use crate::{fframes_logger::FFramesLogger, renderer_error::FFramesRendererResult};
use fframes::{
    media::{decode_image, Subtitles},
    usvgr, DynamicMediaProvider,
};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::{self, Cursor},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// A struct representing owned media directory which can be used to process and fill
/// the media provider. Basically represented as the vector of bytes where every media file is
/// owned bytes vector.
///
/// Video trait implementation provides direct access to the bytes owned by this struct.
pub struct MediaDirectory(Vec<(PathBuf, Vec<u8>)>);

impl MediaDirectory {
    pub(crate) fn read_folder(
        folder_path: impl AsRef<Path>,
    ) -> FFramesRendererResult<MediaDirectory> {
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

    pub(crate) fn into_dynamic_media_provider(
        &self,
        logger: &Arc<dyn FFramesLogger>,
    ) -> FFramesRendererResult<(
        DynamicMediaProvider,
        usvgr_text_layout::fontdb::Database,
        HashMap<String, Arc<usvgr::PreloadedImageData>>,
    )> {
        let audio_hash = Mutex::new(HashMap::new());
        let subtitles_hash = Mutex::new(HashMap::new());
        let image_hash = Mutex::new(HashMap::new());
        let usvgr_image_data = Mutex::new(HashMap::new());
        let fontdb = Mutex::new(usvgr_text_layout::fontdb::Database::new());

        logger.init_media_processing(self.0.len())?;

        self.0
            .par_iter()
            .try_for_each(|(path, bytes)| -> FFramesRendererResult<()> {
                if let Some((extension, filename)) = path
                    .extension()
                    .and_then(OsStr::to_str)
                    .zip(path.file_name().and_then(OsStr::to_str))
                {
                    logger.log_media_processing_start(filename, &path);
                    match extension {
                        "mp3" => {
                            let audio_data = fframes::media::decode_mp3(Cursor::new(bytes))?;
                            audio_hash.lock()?.insert(
                                filename.to_owned(),
                                fframes::AudioData::Preloaded(audio_data),
                            );
                        }
                        "vtt" => {
                            let str_bytes = std::str::from_utf8(&bytes)?;

                            subtitles_hash.lock()?.insert(
                                filename.to_owned(),
                                Subtitles::parse(&str_bytes)
                                    .map_err(fframes::media::FFramesMediaError::from)?,
                            );
                        }
                        "ttf" | "ttc" | "otf" | "otc" => {
                            let data = std::fs::read(path)?;
                            fontdb.lock()?.load_font_file(path);
                        }
                        "jpg" | "jpeg" | "png" => {
                            let data = fs::read(&path)?;

                            usvgr_image_data.lock()?.insert(
                                filename.to_owned(),
                                Arc::new(decode_image(filename, &data)?),
                            );
                        }
                        "DS_Store" => (),
                        _ => {
                            logger.log_unprocessed_media_file(filename);
                        }
                    };
                };

                logger.log_processed_media(&path);
                Ok(())
            })?;

        fontdb.lock()?.load_system_fonts();
        Ok((
            DynamicMediaProvider::new(
                audio_hash.into_inner()?,
                image_hash.into_inner()?,
                subtitles_hash.into_inner()?,
            ),
            fontdb.into_inner()?,
            usvgr_image_data.into_inner()?,
        ))
    }
}
