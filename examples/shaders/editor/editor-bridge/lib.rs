#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::prelude::*;
use shaders_example::{ShadersMedia, ShadersVideo};

impl_wasm_bridge_for!(ShadersVideo<'static>, ShadersMedia);

lazy_static! {
    static ref MEDIA: ShadersMedia =
        ShadersMedia::prepare().expect("Failed static media processing for wasm bridge");
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(ShadersVideo::new(&MEDIA), &MEDIA)
}
