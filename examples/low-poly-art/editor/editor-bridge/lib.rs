#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use low_poly_art_example::{owl, LowPolyMedia, LowPolyVideo};

lazy_static! {
    static ref MEDIA: LowPolyMedia =
        LowPolyMedia::prepare().expect("Failed to create static media");
    static ref OWL_MEDIA: owl::OwlMedia =
        owl::OwlMedia::prepare().expect("Failed to create static media");
    static ref SCENE: owl::Owl<'static> = owl::Owl { media: &OWL_MEDIA };
}

setup_wasm_editor!(
    LowPolyVideo,
    LowPolyVideo {
        media: &MEDIA,
        scene: &*SCENE,
    },
    *OWL_MEDIA
);
