#![feature(async_closure)]
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use beta_example::BetaVideo;

setup_wasm_editor!(BetaVideo, {});
