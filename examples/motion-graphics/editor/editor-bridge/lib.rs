#![cfg(target_arch = "wasm32")]

use fframes_editor_controller::{
    impl_wasm_bridge_for,
    prelude::{lazy_static, *},
};
use motion_graphics_example::{MotionGraphicsMedia, QuoteCardVideo};

impl_wasm_bridge_for!(QuoteCardVideo<'static>, MotionGraphicsMedia);

lazy_static! {
    static ref MEDIA: MotionGraphicsMedia = MotionGraphicsMedia::prepare().unwrap();
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(
        QuoteCardVideo::new(&MEDIA, "PERFORMANCE"),
        &MEDIA,
    )
}
