#![cfg(target_arch = "wasm32")]

use conference_splash_screen::{ConferenceMedia, ConferenceVideo, SpeakerScene, SponsorScene};
use fframes_editor_controller::{
    impl_wasm_bridge_for,
    prelude::{lazy_static, *},
};

impl_wasm_bridge_for!(ConferenceVideo<'static>, ConferenceMedia);

lazy_static! {
    static ref MEDIA: ConferenceMedia = ConferenceMedia::prepare().unwrap();
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(
        ConferenceVideo {
            media: &MEDIA,
            sponsor_scene: SponsorScene { media: &MEDIA },
            speaker_scene: SpeakerScene {
                avatar: Some("antonio.jpg"),
                media: &MEDIA,
                speaker_name: "David Sancho Moreno", 
                talk_title: "My Awesome talk about OCaml and something else to make the title long enough",
                talk_description: "This is a description of the talk that is long enough to wrap into multiple lines. It should be long enough to test the wrapping of the text in the video. More text is needed to make sure the text is wrapped correctly. This is a description of the talk that is long enough to wrap into multiple lines. It should be long enough to test the wrapping of the text in the video. More text is needed to make sure the text is wrapped correctly.",
            },
        },
        &MEDIA
    )
}
