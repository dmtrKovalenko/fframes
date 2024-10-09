use fframes_media_loaders::FFmpegFrame;
use usvgr::PreloadedImageData;

use crate::{
    media::{FFmpegDecoder, ImageData},
    FFramesContext,
};
use std::{
    borrow::{Borrow, BorrowMut},
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
};

pub trait FFramesSyncedVideoFrame {
    fn into_image_data(&self) -> ImageData;
    fn height(&self) -> usize;
    fn width(&self) -> usize;
    fn frame(&self) -> usize;
}

impl FFramesSyncedVideoFrame for FFmpegFrame {
    fn into_image_data(&self) -> ImageData {
        let image_data = unsafe { self.get_image() };

        ImageData {
            filename: "<<in-memory>>".to_owned(),
            image: image_data,
        }
    }

    fn height(&self) -> usize {
        self.height as usize
    }

    fn width(&self) -> usize {
        self.width as usize
    }

    fn frame(&self) -> usize {
        todo!()
    }
}

#[derive(Clone)]
pub struct WorkerLocalDecoders {
    map: Rc<RefCell<HashMap<String, FFmpegDecoder>>>,
}

impl std::fmt::Debug for WorkerLocalDecoders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerLocalDecoders")
            .field("active_decoders", &(*self.map).borrow().len())
            .finish()
    }
}

impl Default for WorkerLocalDecoders {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerLocalDecoders {
    pub fn new() -> Self {
        WorkerLocalDecoders {
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub(crate) fn get_synced_frame<'a>(
        &'a self,
        src_filename: &PathBuf,
        offset: usize,
        ctx: &FFramesContext,
    ) -> crate::error::Result<Option<Arc<impl FFramesSyncedVideoFrame>>> {
        println!("Decoders count    {}", (*self.map).borrow().len());
        let has_decoder = {
            (*self.map)
                .borrow()
                .contains_key(src_filename.to_string_lossy().as_ref())
        };
        if !has_decoder {
            let decoder = unsafe {
                let mut decoder = FFmpegDecoder::new(src_filename, ctx.time_base.fps)?;
                if offset > 0 {
                    decoder.seek_to_offset(offset as i64)?;
                }

                decoder
            };

            println!("Decoders count    {}", (*self.map).borrow().len());
            (*self.map)
                .borrow_mut()
                .insert(src_filename.to_string_lossy().to_string(), decoder);
            println!("Decoders count    {}", (*self.map).borrow().len());
        }

        unsafe {
            let mut decoders = (*self.map).borrow_mut();
            let decoder = decoders
                .get_mut(src_filename.to_string_lossy().as_ref())
                .unwrap();

            let has_frame = decoder.decode_up_to(offset as i64)?;

            if has_frame {
                Ok(Some(decoder.get_raw_frame()))
            } else {
                return Ok(None);
            }
        }
    }
}
