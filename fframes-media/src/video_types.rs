use std::path::PathBuf;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub struct ResizeVideoFrame {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub struct FrameConvertOptions {
    pub resize: ResizeVideoFrame,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GeneralVideoFileMetadata {
    pub width: u32,
    pub height: u32,
    pub duration: f32,
    pub fps: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoMedia {
    pub metadata: Option<GeneralVideoFileMetadata>,
    pub path: PathBuf,
}
