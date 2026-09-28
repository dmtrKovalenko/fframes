#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::prelude::*;
use neon_triangle_example::{NeonTriangleMedia, NeonTriangleVideo};

impl_wasm_bridge_for!(NeonTriangleVideo<'static>, NeonTriangleMedia);

lazy_static! {
    static ref MEDIA: NeonTriangleMedia =
        NeonTriangleMedia::prepare().expect("Failed static media processing for wasm bridge");
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(NeonTriangleVideo::new(&MEDIA), &MEDIA)
}
