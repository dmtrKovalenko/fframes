#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{impl_wasm_bridge_for, prelude::*};
use podcast_example::PodcastVideo;

impl_wasm_bridge_for!(PodcastVideo<'static>);

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(PodcastVideo {
        goose_audio: "final.mp3",
        duck_audio: "final.mp3",
        guest_audio: "final.mp3",
    })
}
