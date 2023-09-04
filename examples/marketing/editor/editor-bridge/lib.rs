use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use marketing_example::{MarketingMedia, MarketingVideo};

lazy_static! {
    static ref MEDIA: MarketingMedia =
        MarketingMedia::prepare().expect("Failed to create static media");
}

setup_wasm_editor!(MarketingVideo, {
  media: &MEDIA,
  audio_track: "marketing.mp3"
}, *MEDIA);
