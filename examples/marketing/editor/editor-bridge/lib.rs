#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{impl_wasm_bridge_for, prelude::*};
use marketing_example::{MarketingMedia, MarketingVideo};

impl_wasm_bridge_for!(MarketingVideo<'static>, MarketingMedia);

lazy_static! {
    static ref MEDIA: MarketingMedia =
        MarketingMedia::prepare().expect("Failed to create static media");
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(
        MarketingVideo {
            media: &MEDIA,
            audio_track: "marketing.mp3",
        },
        &MEDIA,
    )
}
