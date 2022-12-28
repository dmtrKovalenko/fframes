use beta_example::BetaVideo;
use fframes_editor_controller::{prelude::*, setup_wasm_editor};

setup_wasm_editor!(BetaVideo, {
  hours: 14,
  minutes: 4,
});
