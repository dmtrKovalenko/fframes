#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use podcast_example::PodcastVideo;

setup_wasm_editor!(PodcastVideo, {
  goose_audio: "final.mp3",
  duck_audio: "final.mp3",
  guest_audio: "final.mp3"
}, ());
