mod mp3;
use fframes::{
    usvgr, AudioData, PreloadedAudioData, Subtitles, {ImageData, MediaProvider},
};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
};

use crate::{
    fframes_logger::FFramesLogger,
    renderer_error::{FFramesRendererError, FFramesRendererResult},
};

pub(crate) fn load_media_from_folder(
    logger: &Arc<dyn FFramesLogger>,
    folder_path: &str,
    fps: usize,
) -> FFramesRendererResult<(
    MediaProvider,
    usvgr_text_layout::fontdb::Database,
    HashMap<String, Arc<usvgr::PreloadedImageData>>,
)> {
    let audio_hash = Mutex::new(HashMap::new());
    let subtitles_hash = Mutex::new(HashMap::new());
    let image_hash = Mutex::new(HashMap::new());
    let usvgr_image_data = Mutex::new(HashMap::new());
    let fontdb = Mutex::new(usvgr_text_layout::fontdb::Database::new());

    let folder_path = Path::new(folder_path);
    if !folder_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput, // todo change to NotADirectory when this https://github.com/rust-lang/rust/issues/86442 will be stable
            "resources_dir must be a folder",
        )
        .into());
    }

    let media_files = fs::read_dir(folder_path)?
        .filter_map(|path_buf| {
            path_buf.ok().and_then(|path_buf| {
                let path = path_buf.path();
                if path.is_dir() {
                    None
                } else {
                    Some(path)
                }
            })
        })
        .collect::<Vec<_>>();

    logger.init_media_processing(media_files.len());

    media_files
        .into_par_iter()
        .try_for_each(|path| -> FFramesRendererResult<()> {
            if let Some((extension, filename)) = path
                .extension()
                .and_then(OsStr::to_str)
                .zip(path.file_name().and_then(OsStr::to_str))
            {
                logger.log_media_processing_start(filename, &path);

                match extension {
                    "mp3" => {
                        audio_hash.lock()?.insert(
                            filename.to_owned(),
                            AudioData::Preloaded(mp3::decode_mp3(&path)?),
                        );
                    }
                    "vtt" => {
                        subtitles_hash
                            .lock()?
                            .insert(filename.to_owned(), Subtitles::from_file(&path, fps)?);
                    }
                    "ttf" | "ttc" | "otf" | "otc" => {
                        if let Some(font_path) = path.to_str() {
                            let data = std::fs::read(font_path)?;
                            fontdb.lock()?.load_font_data(data);
                        }
                    }
                    "jpg" | "jpeg" | "png" => {
                        let data = fs::read(&path)?;
                        let buffer = image::load_from_memory(data.as_slice()).map_err(|e| {
                            FFramesRendererError::ImageError((filename.to_owned(), e))
                        })?;

                        usvgr_image_data.lock()?.insert(
                            filename.to_owned(),
                            usvgr::PreloadedImageData::new(
                                if filename.ends_with(".png") {
                                    "png".to_owned()
                                } else {
                                    "jpeg".to_owned()
                                },
                                buffer.width(),
                                buffer.height(),
                                buffer.to_rgba8().into_raw(),
                            ),
                        );

                        image_hash.lock()?.insert(
                            filename.to_owned(),
                            ImageData {
                                link: filename.to_owned(),
                                base64: None,
                            },
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
        MediaProvider {
            audio: audio_hash.into_inner()?,
            images: image_hash.into_inner()?,
            subtitles: subtitles_hash.into_inner()?,
        },
        fontdb.into_inner()?,
        usvgr_image_data.into_inner()?,
    ))
}
