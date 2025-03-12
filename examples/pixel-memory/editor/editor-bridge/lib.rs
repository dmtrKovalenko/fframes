#![cfg(target_arch = "wasm32")]

use fframes_editor_controller::{
    prelude::{lazy_static, *},
    setup_wasm_editor,
};
use hello_world_example::{rand, PixelMedia, PixelVideo, RandomPhotos};

lazy_static! {
    static ref MEDIA: PixelMedia = PixelMedia::prepare().unwrap();
    static ref PHOTOS_LIST: Vec<String> = (2..=235).map(|i| format!("{i}.jpg")).collect();
    static ref VIDEOS_LIST: Vec<String> = (1..=22).map(|i| format!("{i}.mp4")).collect();
}

setup_wasm_editor!(
    PixelVideo,
    PixelVideo::new_random_scenes(
        "The Farewell.mp3",
        &mut rand::thread_rng(),
        None::<&()>,
        RandomPhotos::new_from_static_list(
            &mut rand::thread_rng(),
            PHOTOS_LIST.as_slice(),
            VIDEOS_LIST.as_slice()
        )
    ),
    *MEDIA
);
