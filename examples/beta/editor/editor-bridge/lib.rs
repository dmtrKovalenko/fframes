#![cfg(target_arch = "wasm32")]
use beta_example::BetaVideo;
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use tiktok_media::GooseMedia;

lazy_static! {
    static ref MEDIA: GooseMedia = GooseMedia::prepare().unwrap();
}

setup_wasm_editor!(BetaVideo, {
  hours: 14,
  minutes: 4,
  tiktok_media: &MEDIA,
}, ());
