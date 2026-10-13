use crate::{FramedImage, PhotoFrame, PixelVideo, pixel_video_randomizer::RandomPhotos};
use fframes::{
    Scene, Svgr, Transform, Video,
    animation::{Easing, KeyFrame, KeyFramesAnimation},
};
use rand::Rng;
use std::fmt::{Debug, Formatter, Result as FmtResult};

pub struct ParallaxGridPhotos<'a> {
    duration: f32,
    main_animation: KeyFramesAnimation<f32>,
    main_photos: Vec<&'a str>,
    bg_animation: KeyFramesAnimation<f32>,
    bg_photos: Vec<&'a str>,
}

impl Scene for ParallaxGridPhotos<'_> {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Next(0.3)
    }

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(self.duration)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        const ROW_HEIGHT: usize = 400;
        const PHOTO_HEIGHT: f64 = 350.0;

        let bg_position = frame.animate(&self.bg_animation);
        fframes::svgr!(
            <g
                id="bg_parallax_images"
                transform={Transform { translate_y: f64::from(-bg_position), ..Default::default() }}
            >
                {self.bg_photos.iter().enumerate().filter_map(|(index, photo)| {
                    let image = ctx.get_image(photo)?;
                    let scale_ratio = f64::from(image.metadata.height) / PHOTO_HEIGHT;
                    let width = f64::from(image.metadata.width) / scale_ratio;

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
                            transform={Transform {
                                translate_x: -width / 2.0,
                                scale: 1.2.into(),
                                ..Default::default()
                            }}
                            opacity={0.85}
                        />
                    ))
                }).collect::<Svgr>()}
            </g>

            <g
              id="main_parallax_images"
              transform={
                Transform {
                    translate_y: f64::from(-frame.animate(&self.main_animation)),
                    ..Default::default()
                }}
              >
                {self.main_photos.iter().enumerate().filter_map(|(index, photo)| {
                    const PHOTO_HEIGHT: f32 = 800.0;

                    let image = ctx.get_image(photo)?;
                    let scale_ratio = image.metadata.height as f32 / PHOTO_HEIGHT;
                    let width = image.metadata.width as f32 / scale_ratio;

                    Some(
                        image.render_framed(PhotoFrame {
                            x: PixelVideo::WIDTH as f32 / 2.,
                            y: 1080. * index as f32 + (1080. - PHOTO_HEIGHT) / 2.,
                            width: width as u32,
                            height: PHOTO_HEIGHT as u32,
                            stroke_width: 48,
                            transform: Transform {
                                translate_x: f64::from(-width) / 2.,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                    )
                }).collect::<Svgr>()}
            </g>
        )
    }
}

impl<'a> ParallaxGridPhotos<'a> {
    pub fn create(
        main_photos: &[&'a str],
        bg_photos: &[&'a str],
        tempo: f32,
        rng: &mut impl Rng,
    ) -> Self {
        let main_photos = main_photos.to_vec();
        let bg_photos = bg_photos.to_vec();

        let pause = rng.gen_range(0.2..=0.3);
        let enter_duration = tempo * rng.gen_range(1.5..=2.5);

        let mut main_keyframes = main_photos
            .iter()
            .enumerate()
            .map(|(index, _)| KeyFrame {
                start: index as f32 * (tempo * 4.) + pause + enter_duration,
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
                end: Some(enter_duration - pause),
                from: -1080.,
                to: 0.,
                easing: &Easing::EaseOut,
            },
        );

        let main_animation = KeyFramesAnimation::new(main_keyframes);
        let bg_animation = KeyFramesAnimation::new(vec![KeyFrame {
            start: 0.,
            end: Some(main_animation.total_duration + rng.gen_range(0.3..=0.7)),
            from: -1080.,
            to: (bg_photos.len() + 2) as f32 * 400.,
            easing: &Easing::Linear,
        }]);

        Self {
            duration: bg_animation.total_duration - 0.5,
            main_photos,
            main_animation,
            bg_photos,
            bg_animation,
        }
    }

    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let main_length = rng.gen_range(2..=4);
        let main_photos = images.choose(main_length);
        let bg_photos = images.choose(main_photos.len() * 3);
        Self::create(&main_photos, &bg_photos, tempo, rng)
    }
}

impl Debug for ParallaxGridPhotos<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let mut photos = self.main_photos.clone();
        photos.extend_from_slice(&self.bg_photos);
        write!(f, "ParallaxGridPhotos: {}", photos.join(", "))
    }
}
