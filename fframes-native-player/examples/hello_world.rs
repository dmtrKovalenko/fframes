//! `cargo run --release -p fframes_native_player --example hello_world`
use fframes::StaticMediaProvider;
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};

fn main() {
    let media = HelloWorldMedia::prepare().expect("static media");
    let video = HelloWorldVideo {
        media: &media,
        slug: "Native player!",
    };

    fframes_native_player::play(
        &video,
        &fframes_native_player::PlayerOptions {
            media: Some(&media),
            title: "hello-world",
            ..Default::default()
        },
    )
    .unwrap();
}
