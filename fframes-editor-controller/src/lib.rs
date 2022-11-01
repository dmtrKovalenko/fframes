mod setup_wasm_editor;
pub use setup_wasm_editor::*;

pub mod prelude {
    pub use console_error_panic_hook;
    pub use fframes::{fframes_context, frame, subtitles::Subtitles, video::Video, AudioTimestamp};
    pub use js_sys;
    pub use lazy_static::lazy_static;
    pub use serde;
    pub use std::{collections::HashMap, sync::Mutex};
    pub use ttf_parser;
    pub use wasm_bindgen;
    pub use wasm_bindgen::prelude::*;
    pub use wasm_bindgen_futures;
}
