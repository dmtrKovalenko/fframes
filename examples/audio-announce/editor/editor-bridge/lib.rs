#![cfg(target_arch = "wasm32")]

use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};

lazy_static! {
    static ref MEDIA: AudioAnnounceMedia = AudioAnnounceMedia::prepare().unwrap();
}

setup_wasm_editor!(
    AudioAnnounce,
    AudioAnnounce {
        media: &MEDIA,
        font: None
    },
    *MEDIA
);
