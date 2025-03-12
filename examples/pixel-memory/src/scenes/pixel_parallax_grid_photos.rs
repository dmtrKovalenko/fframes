use crate::{FramedImage, PhotoFrame, pixel_video_randomizer::RandomPhotos};
use fframes::{
    Scene, Svgr,
    animation::{Easing, KeyFrame, KeyFramesAnimation},
};
use rand::Rng;

#[derive(Debug)]
pub struct ParallaxGridPhotos<'a> {
    duration: f32,
    main_animation: KeyFramesAnimation<f32>,
    main_photos: Vec<&'a str>,
    bg_animation: KeyFramesAnimation<f32>,
    bg_photos: Vec<&'a str>,
}

impl Scene for ParallaxGridPhotos<'_> {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Next(0.5)
    }

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(self.duration)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        const ROW_HEIGHT: usize = 400;
        const PHOTO_HEIGHT: f32 = 350.0;

        let bg_position = frame.animate(&self.bg_animation);
        fframes::svgr!(
            <g transform={format!("translate(0, {})", -bg_position)}>
                {self.bg_photos.iter().enumerate().filter_map(|(index, photo)| {
                    let image = ctx.get_image(photo)?;
                    let scale_ratio = image.metadata.height as f32 / PHOTO_HEIGHT;
                    let width = image.metadata.width as f32 / scale_ratio;

                    Some(fframes::svgr!(
                        <image
                            href={image.href()}
                            height={PHOTO_HEIGHT}
                            width={width}
                            x={if index % 2 == 0 {
                                "75%"
                            } else {
                                "25%"
                            }}
                            y={if index == 0 {
                                ROW_HEIGHT / 3
                            } else {
                                index * ROW_HEIGHT
                            }}
                            transform-origin="center center"
                            transform={format!("translate(-{}) scale(1.2)", width / 2.)}
                            opacity={0.85}
                        />
                    ))
                }).collect::<Svgr>()}
            </g>

            <g transform={format!("translate(0, {})", -frame.animate(&self.main_animation))}>
                {self.main_photos.iter().enumerate().filter_map(|(index, photo)| {
                    const PHOTO_HEIGHT: f32 = 800.0;

                    let image = ctx.get_image(photo)?;
                    let scale_ratio = image.metadata.height as f32 / PHOTO_HEIGHT;
                    let width = image.metadata.width as f32 / scale_ratio;

                    Some(
                        image.render_framed(PhotoFrame {
                            x: ctx.current_video_size.width as f32 / 2.,
                            y: 1080. * index as f32 + (1080. - PHOTO_HEIGHT) / 2.,
                            width: width as u32,
                            height: PHOTO_HEIGHT as u32,
                            stroke_width: 48,
                            transform: format!("translate(-{})", width / 2.),
                            ..Default::default()
                        })
                    )
                }).collect::<Svgr>()}
            </g>
        )
    }
}

impl<'a> ParallaxGridPhotos<'a> {
    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let main_length = rng.gen_range(2..=4);
        let main_photos = images.choose(main_length);
        let bg_photos = images.choose(main_photos.len() * 3);

        let pause = rng.gen_range(0.1..=0.2);
        let enter_duration = tempo * rng.gen_range(1.5..=3.5);

        let mut main_keyframes = main_photos
            .iter()
            .enumerate()
            .map(|(index, _)| KeyFrame {
                start: index as f32 * (tempo * 4. + pause) + enter_duration,
                end: Some((index + 1) as f32 * tempo * 4. - pause),
                from: index as f32 * 1080.,
                to: (index + 1) as f32 * 1080.,
                easing: &Easing::EaseInOut,
            })
            .collect::<Vec<_>>();

        main_keyframes.insert(
            0,
            KeyFrame {
                start: 0.,
                end: Some(enter_duration),
                from: -1080.,
                to: 0.,
                easing: &Easing::EaseOut,
            },
        );

        let main_animation = KeyFramesAnimation::new(main_keyframes);
        let bg_animation = KeyFramesAnimation::new(vec![
            KeyFrame {
                start: 0.,
                end: Some(enter_duration),
                from: -1080.,
                to: 0.,
                easing: &Easing::Linear,
            },
            KeyFrame {
                start: enter_duration,
                end: Some(main_animation.total_duration + rng.gen_range(0.3..=0.7)),
                from: 0.,
                to: (bg_photos.len() + 2) as f32 * 400.,
                easing: &Easing::Linear,
            },
        ]);

        Self {
            duration: bg_animation.total_duration - 0.5,
            main_photos,
            main_animation,
            bg_photos,
            bg_animation,
        }
    }
}
