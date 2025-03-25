#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{
    impl_wasm_bridge_for,
    prelude::{lazy_static, *},
};
use tiktok_example::{GooseMedia, GooseVideo};

impl_wasm_bridge_for!(GooseVideo<'static>, GooseMedia);

lazy_static! {
    static ref MEDIA: GooseMedia = GooseMedia::prepare().unwrap();
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(GooseVideo { media: &MEDIA }, &MEDIA)
}
