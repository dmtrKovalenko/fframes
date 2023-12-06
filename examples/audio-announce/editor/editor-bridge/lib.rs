#![cfg(target_arch = "wasm32")]

use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};
use hello_world_example::{AudioAnnounce, AudioAnnounceMedia};

lazy_static! {
    static ref MEDIA: AudioAnnounceMedia = AudioAnnounceMedia::prepare().unwrap();
}

setup_wasm_editor!(AudioAnnounce, { media: &MEDIA, font: None }, *MEDIA);
