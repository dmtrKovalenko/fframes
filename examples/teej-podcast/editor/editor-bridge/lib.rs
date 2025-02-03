#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use teej_podcast_example::{Chapter, TeejPodcast};

setup_wasm_editor!(
    TeejPodcast,
    TeejPodcast::new(&[
        Chapter {
            title: "Introduction to Lunch Bites Podcast",
            start: "00:00",
        },
        Chapter {
            title: "Tech Sponsorships and Streaming Quality",
            start: "01:51",
        },
        Chapter {
            title: "Flexing in LA: The Casa Bonita Experience",
            start: "02:33",
        },
        Chapter {
            title: "Viral Moments: The Post-It Note Debate",
            start: "03:00",
        },
        Chapter {
            title: "Zuckerberg's Rebranding and Tech Culture",
            start: "04:51",
        },
        Chapter {
            title: "The Intersection of Geek Culture and Popularity",
            start: "14:55",
        },
        Chapter {
            title: "Psychedelic Fantasy Baseball and AI",
            start: "16:48",
        },
        Chapter {
            title: "The Rise of Meme Coins",
            start: "17:52",
        },
        Chapter {
            title: "Trump Coin and the Crypto Circus",
            start: "19:13",
        },
        Chapter {
            title: "Fart Coin: The AI Millionaire",
            start: "21:50",
        },
        Chapter {
            title: "OpenAI and the Future of AI Models",
            start: "25:46",
        },
        Chapter {
            title: "Influencers and the Coding Landscape",
            start: "30:10",
        },
    ])
);
