use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use hello_world_example::HelloWorldVideo;

pub fn test() {
    let _a = fframes::Svgr {
        value: "".to_string(),
    };
}

setup_wasm_editor!(HelloWorldVideo, { slug: "hey" });
