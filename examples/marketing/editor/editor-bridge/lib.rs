use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use marketing_example::MarketingVideo;

setup_wasm_editor!(MarketingVideo, {
  audio_track: "marketing.mp3"
});
