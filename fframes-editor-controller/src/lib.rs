#![cfg(target_arch = "wasm32")]
pub mod impl_wasm_bridge_macro;

mod video_metadata;
pub mod wasm_audio;
mod wasm_editor;
pub mod wasm_font_source;

pub use fframes;
pub use lazy_static;
pub use wasm_editor::*;

pub mod prelude {
    pub use crate::video_metadata::VideoMetadata;
    pub use crate::wasm_audio;
    pub use crate::wasm_font_source;
    pub use crate::{impl_wasm_bridge_for, internal_impl_wasm_bridge};
    pub use console_error_panic_hook;
    pub use fframes;
    pub use fframes::StaticMediaProvider;
    pub use lazy_static::lazy_static;
    pub use serde_wasm_bindgen;
    pub use std::convert::TryInto;
    pub use std::{
        collections::HashMap,
        sync::{Mutex, RwLock},
    };
    pub use wasm_bindgen;
    pub use wasm_bindgen::prelude::*;
    pub use wasm_bindgen_futures;
}
