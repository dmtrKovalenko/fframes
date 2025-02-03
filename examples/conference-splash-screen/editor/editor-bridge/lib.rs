#![cfg(target_arch = "wasm32")]

use conference_splash_screen::{ConferenceMedia, ConferenceVideo, SpeakerScene, SponsorScene};
use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};

lazy_static! {
    static ref MEDIA: ConferenceMedia = ConferenceMedia::prepare().unwrap();
}

setup_wasm_editor!(ConferenceVideo, ConferenceVideo {
    media: &MEDIA,
    sponsor_scene: SponsorScene { media: &MEDIA },
    speaker_scene: SpeakerScene {
        avatar: Some("antonio.jpg"),
        media: &MEDIA,
        speaker_name: "David Sancho Moreno", 
        talk_title: "My Awesome talk about OCaml and something else to make the title long enough",
        talk_description: "This is a description of the talk that is long enough to wrap into multiple lines. It should be long enough to test the wrapping of the text in the video. More text is needed to make sure the text is wrapped correctly. This is a description of the talk that is long enough to wrap into multiple lines. It should be long enough to test the wrapping of the text in the video. More text is needed to make sure the text is wrapped correctly.",
    },
}, *MEDIA);
