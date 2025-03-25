#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{impl_wasm_bridge_for, prelude::*};
use low_poly_art_example::{owl, LowPolyMedia, LowPolyVideo};
use std::ops::Deref;

impl_wasm_bridge_for!(LowPolyVideo<'static>, owl::OwlMedia);

lazy_static! {
    static ref MEDIA: LowPolyMedia =
        LowPolyMedia::prepare().expect("Failed to create static media");
    static ref OWL_MEDIA: owl::OwlMedia =
        owl::OwlMedia::prepare().expect("Failed to create static media");
    static ref SCENE: owl::Owl<'static> = owl::Owl { media: &OWL_MEDIA };
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(
        LowPolyVideo {
            media: &MEDIA,
            scene: SCENE.deref(),
        },
        &OWL_MEDIA,
    )
}
