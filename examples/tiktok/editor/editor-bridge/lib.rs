#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};
use tiktok_example::{GooseMedia, GooseVideo};

lazy_static! {
    static ref MEDIA: GooseMedia = GooseMedia::prepare().unwrap();
}

setup_wasm_editor!(GooseVideo, { media: &MEDIA }, *MEDIA);
