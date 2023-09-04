mod setup_wasm_editor;
pub use setup_wasm_editor::*;

pub mod video_metadata;
pub mod wasm_audio;
pub mod wasm_font_source;

pub mod prelude {
    pub use crate::{video_metadata, wasm_audio, wasm_font_source};
    pub use console_error_panic_hook;
    pub use fframes;
    pub use fframes::lru;
    pub use fframes::media::{ImageData, Subtitles};
    pub use fframes::serde;
    pub use fframes::ttf_parser;
    pub use fframes::{
        AudioTimestamp, FFramesContext, FFramesMode, Frame, MediaProvider, StaticMediaProvider,
        Video,
    };
    pub use js_sys;
    pub use lazy_static::lazy_static;
    pub use serde_wasm_bindgen;
    pub use std::{
        collections::HashMap,
        sync::{Mutex, RwLock},
    };
    pub use wasm_bindgen;
    pub use wasm_bindgen::prelude::*;
    pub use wasm_bindgen_futures;
}
