#![feature(async_closure)]
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use tiktok_example::GooseVideo;

setup_wasm_editor!(GooseVideo, {
  audio_track: "thought.mp3"
});
