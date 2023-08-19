use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};

setup_wasm_editor!(HelloWorldVideo, { media: HelloWorldMedia::prepare().unwrap(), slug: "Hello World!" });
