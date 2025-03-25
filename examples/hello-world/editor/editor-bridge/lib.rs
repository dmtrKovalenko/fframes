#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::prelude::*;
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};

impl_wasm_bridge_for!(HelloWorldVideo<'static>, HelloWorldMedia);

lazy_static! {
    static ref MEDIA: HelloWorldMedia =
        HelloWorldMedia::prepare().expect("Failed static media processing for wasm bridge");
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(
        HelloWorldVideo {
            media: &MEDIA,
            slug: "World",
        },
        &MEDIA,
    )
}
