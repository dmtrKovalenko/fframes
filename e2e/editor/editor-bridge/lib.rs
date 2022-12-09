#![feature(async_closure)]
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use hello_world_example::HelloWorldMultiSceneVideo;

setup_wasm_editor!(HelloWorldMultiSceneVideo, {});
