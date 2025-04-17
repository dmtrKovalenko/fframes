use fframes::{MediaProvider, Scene, lazy_static::lazy_static};
use rand::{Rng, seq::SliceRandom};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use crate::{
    PixelVideo,
    bokeh_background::generate_bokeh,
    scenes::{
        FibonacciSpiralGallery, FinalScene, ParallaxGridPhotos, PixelSingleVideoScene,
        PolaroidDevelopment, SinglePhotoFloat, SpiralHeapGallery, StartScene,
    },
};

#[derive(Debug, Clone, Copy)]
struct SongInfo {
    duration: f32,
    tempo: f32,
}

lazy_static! {
    static ref SONGS: HashMap<&'static str, SongInfo> = HashMap::from([
        (
            "The_Farewell.mp3",
            SongInfo {
                duration: 166.0,
                tempo: 1.12
            }
        ),
        (
            "vostok_zapomny.mp3",
            SongInfo {
                duration: 185.62,
                tempo: 1.08
            }
        ),
        (
            "naruto_grief.mp3",
            SongInfo {
                duration: 196.,
                tempo: 1.22
            }
        ),
        (
            "revenge.mp3",
            SongInfo {
                duration: 132.,
                tempo: 1.05
            }
        ),
        (
            "passenger.mp3",
            SongInfo {
                duration: 189.,
                tempo: 1.2
            }
        )
    ]);
    pub static ref ALL_SONGS: Vec<&'static str> = SONGS.keys().copied().collect();
}

impl<'a> PixelVideo<'a> {
    fn randomize_scenes(
        total_duration: f32,
        tempo: f32,
        rng: &mut impl Rng,
        enter_text: &'a str,
        media_provider: Option<&'a impl fframes::MediaProvider<'a>>,
        images: &mut RandomPhotos<'a>,
    ) -> Vec<Arc<dyn Scene + 'a>> {
        let get_duration = |scene: &dyn Scene| -> Option<f32> {
            let duration = match (scene.duration(), media_provider) {
                (fframes::Duration::Seconds(duration), _) => Some(duration),
                (fframes::Duration::FromAudio(audio), Some(provider)) => {
                    let audio = provider.resolve_audio(audio)?;
                    Some(audio.duration_in_seconds())
                }
                (fframes::Duration::FromAudio(_), None) => {
                    Some(9.0) // for editor fallback
                }
                _ => None,
            }?;

            let overlap = match scene.overlap() {
                fframes::Overlap::Previous(seconds) | fframes::Overlap::Next(seconds) => seconds,
                fframes::Overlap::PreviousAndNext { previous, next } => previous - next + next,
                fframes::Overlap::None => 0.,
            };

            Some(duration - overlap)
        };

        let mut current_duration = tempo * rng.gen_range(4..=5) as f32;
        let mut scenes = vec![Arc::new(StartScene {
            duration: current_duration,
            text: enter_text,
        }) as Arc<dyn Scene + 'a>];

        current_duration -= 3.0;

        while (total_duration - current_duration) > 10. {
            let chance = rng.gen_range(if total_duration - current_duration > 20. {
                0..50
            } else {
                0..20
            });

            let new_scene =
                match chance {
                    0..10 => Arc::new(SinglePhotoFloat::generate(rng, tempo, images))
                        as Arc<dyn Scene + 'a>,
                    10..20 => Arc::new(FibonacciSpiralGallery::generate(rng, tempo, images))
                        as Arc<dyn Scene + 'a>,
                    20..25 => Arc::new(PolaroidDevelopment::generate(rng, tempo, images))
                        as Arc<dyn Scene + 'a>,
                    25..30 => Arc::new(SpiralHeapGallery::generate(rng, tempo, images))
                        as Arc<dyn Scene + 'a>,
                    30..40 => Arc::new(ParallaxGridPhotos::generate(rng, tempo, images))
                        as Arc<dyn Scene + 'a>,
                    40..50 => {
                        let video = images.choose_video();
                        Arc::new(PixelSingleVideoScene { video }) as Arc<dyn Scene + 'a>
                    }
                    _ => panic!("unhandled scene chance invariance"),
                };

            let Some(duration) = get_duration(new_scene.as_ref()) else {
                fframes::log!("No duration found for scene, skipping");
                continue;
            };
            // left at minimum 3 seconds for the final scene
            if current_duration + duration < total_duration - 3. {
                scenes.push(new_scene);
                current_duration += duration;
            }
        }

        scenes.push(Arc::new(FinalScene::new(total_duration - current_duration)));

        scenes
    }

    pub fn new_random_scenes(
        song: &'a str,
        text: &'a str,
        rng: &mut impl Rng,
        provider: Option<&'a impl MediaProvider<'a>>,
        mut images: RandomPhotos<'a>,
    ) -> Self {
        let bokeh_circles = generate_bokeh(rng, 120..140);
        let SongInfo {
            duration: total_duration,
            tempo,
        } = *SONGS.get(song).expect("Selected song not available.");

        Self {
            total_duration,
            bokeh_circles,
            music: song,
            scenes: Self::randomize_scenes(total_duration, tempo, rng, text, provider, &mut images),
        }
    }
}

#[derive(Debug)]
pub struct RandomPhotos<'a> {
    all_images: VecDeque<&'a str>,
    all_videos: VecDeque<&'a str>,
}

impl<'a> RandomPhotos<'a> {
    pub fn choose_video(&mut self) -> &'a str {
        self.all_videos
            .pop_front()
            .expect("No more videos found, add more to the videos folder")
    }

    pub fn choose_one(&mut self) -> &'a str {
        self.all_images
            .pop_front()
            .expect("No more images found, add more to the photos folder")
    }

    pub fn choose(&mut self, n: usize) -> Vec<&'a str> {
        self.all_images.drain(..n).collect()
    }

    pub fn new_from_media_provider(
        rng: &mut impl Rng,
        source: &'a impl fframes::MediaProvider<'a>,
    ) -> Self {
        let mut images: Vec<_> = source
            .get_all_image_data()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        images.shuffle(rng);

        let mut videos: Vec<_> = source
            .get_all_video_data()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        videos.shuffle(rng);

        Self {
            all_images: VecDeque::from(images),
            all_videos: VecDeque::from(videos),
        }
    }

    pub fn new_from_static_list(
        rng: &mut impl Rng,
        images: &'static [String],
        videos: &'static [String],
    ) -> Self {
        let mut images = images.iter().map(|name| name.as_str()).collect::<Vec<_>>();
        images.shuffle(rng);

        let mut videos = videos.iter().map(|name| name.as_str()).collect::<Vec<_>>();
        videos.shuffle(rng);

        Self {
            all_images: VecDeque::from(images),
            all_videos: VecDeque::from(videos),
        }
    }
}
