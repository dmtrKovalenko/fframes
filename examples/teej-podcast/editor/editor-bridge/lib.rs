#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{impl_wasm_bridge_for, prelude::*};
use teej_podcast_example::{Chapter, TeejPodcast};

impl_wasm_bridge_for!(TeejPodcast<'static>);

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();

    WasmBridge::new(TeejPodcast::new(&[
        Chapter::new("Introduction to Lunch Bites Podcast", "00:00"),
        Chapter::new("Tech Sponsorships and Streaming Quality", "01:51"),
        Chapter::new("Flexing in LA: The Casa Bonita Experience", "02:33"),
        Chapter::new("Viral Moments: The Post-It Note Debate", "03:00"),
        Chapter::new("Zuckerberg's Rebranding and Tech Culture", "04:51"),
        Chapter::new("The Intersection of Geek Culture and Popularity", "14:55"),
        Chapter::new("Psychedelic Fantasy Baseball and AI", "16:48"),
        Chapter::new("The Rise of Meme Coins", "17:52"),
        Chapter::new("Trump Coin and the Crypto Circus", "19:13"),
        Chapter::new("Fart Coin: The AI Millionaire", "21:50"),
        Chapter::new("OpenAI and the Future of AI Models", "25:46"),
        Chapter::new("Influencers and the Coding Landscape", "30:10"),
    ]))
}
