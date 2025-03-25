#![cfg(target_arch = "wasm32")]

use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
use fframes_editor_controller::{
    impl_wasm_bridge_for,
    prelude::{lazy_static, *},
};

impl_wasm_bridge_for!(AudioAnnounce<'static>, AudioAnnounceMedia);

lazy_static! {
    static ref MEDIA: AudioAnnounceMedia = AudioAnnounceMedia::prepare().unwrap();
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(
        AudioAnnounce {
            media: &MEDIA,
            font: None,
        },
        &MEDIA,
    )
}
