#![cfg(target_arch = "wasm32")]

use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};

lazy_static! {
    static ref MEDIA: HelloWorldMedia = HelloWorldMedia::prepare().unwrap();
}

setup_wasm_editor!(HelloWorldVideo, { media: &MEDIA, slug: "Hello World!" }, *MEDIA);
