#![cfg(target_arch = "wasm32")]

use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};
use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};

lazy_static! {
    static ref MEDIA: AudioAnnounceMedia = AudioAnnounceMedia::prepare().unwrap();
}

setup_wasm_editor!(AudioAnnounce, { media: &MEDIA, font: None }, *MEDIA);
