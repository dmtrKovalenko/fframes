use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use hello_world_example::HelloWorldVideo;

setup_wasm_editor!(HelloWorldVideo, { slug: "Hello World!" });
