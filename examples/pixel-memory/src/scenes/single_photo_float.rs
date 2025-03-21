use crate::{FramedImage, PhotoFrame, PixelVideo, RandomPhotos};
use fframes::{
    Scene, Svgr, Transform, Video,
    animation::{Easing, KeyFrame, KeyFramesAnimation},
};
use rand::Rng;

#[derive(Debug)]
pub struct SinglePhotoFloat<'a> {
    duration: f32,
    photo: &'a str,
    transform_animation: KeyFramesAnimation<Transform>,
    rotation_animation: KeyFramesAnimation<f32>,
}

const BASE_PHOTO_SIZE: f32 = 700.0;

impl Scene for SinglePhotoFloat<'_> {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Next(0.4)
    }

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(self.duration)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        let Some(image) = ctx.get_image(self.photo) else {
            return Svgr::empty();
        };

        let original_width = image.metadata.width as f32;
        let original_height = image.metadata.height as f32;
        let aspect_ratio = original_width / original_height;

        let photo_width = if aspect_ratio > 1.0 {
            BASE_PHOTO_SIZE
        } else {
            BASE_PHOTO_SIZE * aspect_ratio
        } as f64;

        let photo_height = if aspect_ratio > 1.0 {
            BASE_PHOTO_SIZE / aspect_ratio
        } else {
            BASE_PHOTO_SIZE
        } as f64;

        let mut transform = frame.animate(&self.transform_animation);
        transform.rotate = frame.animate(&self.rotation_animation).into();
        transform.translate_x -= photo_width / 2.0;
        transform.translate_y -= photo_height / 2.0;

        image.render_framed(PhotoFrame {
            x: 0.0, // we use transform
            y: 0.0,
            width: photo_width as u32,
            height: photo_height as u32,
            stroke_width: 48,
            transform,
            transform_origin: "center center".to_string(),
            ..Default::default()
        })
    }
}

impl<'a> SinglePhotoFloat<'a> {
    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let photo = images.choose_one();

        let direction_horizontal = rng.gen_bool(0.5);
        let entry_duration = tempo * 3.0;
        let center_duration = tempo * 4.0;
        let exit_duration = tempo * 3.0;
        let total_duration = entry_duration + center_duration + exit_duration;
        let exit_start_time = entry_duration + center_duration;

        let video_width = PixelVideo::WIDTH as f32;
        let video_height = PixelVideo::HEIGHT as f32;
        let center_x = video_width / 2.0;
        let center_y = video_height / 2.0;

        let (start_x, start_y, exit_x, exit_y) = if direction_horizontal {
            (video_width + 200.0, center_y, -BASE_PHOTO_SIZE, center_y)
        } else {
            (center_x, video_height + 200.0, center_x, -BASE_PHOTO_SIZE)
        };

        let main_position = (
            center_x + rng.gen_range(-30.0..30.0),
            center_y + rng.gen_range(-30.0..30.0),
        );

        let main_scale = rng.gen_range(1.0..=1.3);
        let transform_animation = KeyFramesAnimation::new(vec![
            KeyFrame {
                start: 0.0,
                end: Some(entry_duration),
                from: Transform {
                    translate_x: start_x.into(),
                    translate_y: start_y.into(),
                    scale: 0.9.into(),
                    ..Default::default()
                },
                to: Transform {
                    translate_x: center_x.into(),
                    translate_y: center_y.into(),
                    scale: 1.0.into(),
                    ..Default::default()
                },
                easing: &Easing::EaseOut,
            },
            KeyFrame {
                start: entry_duration,
                end: Some(exit_start_time),
                from: Transform {
                    translate_x: center_x.into(),
                    translate_y: center_y.into(),
                    scale: 1.0.into(),
                    ..Default::default()
                },
                to: Transform {
                    translate_x: main_position.0.into(),
                    translate_y: main_position.1.into(),
                    scale: main_scale.into(),
                    ..Default::default()
                },
                easing: &Easing::EaseInOut,
            },
            KeyFrame {
                start: exit_start_time,
                end: Some(total_duration),
                from: Transform {
                    translate_x: main_position.0.into(),
                    translate_y: main_position.1.into(),
                    scale: main_scale.into(),
                    ..Default::default()
                },
                to: Transform {
                    translate_x: exit_x.into(),
                    translate_y: exit_y.into(),
                    scale: rng.gen_range(0.7..0.9).into(),
                    ..Default::default()
                },
                easing: &Easing::EaseIn,
            },
        ]);

        let main_rotate = rng.gen_range(-2.0..2.0);
        let rotation_animation = KeyFramesAnimation::new(vec![
            KeyFrame {
                start: 0.0,
                end: Some(entry_duration),
                from: rng.gen_range(-5.0..5.0),
                to: 0.0,
                easing: &Easing::EaseOut,
            },
            KeyFrame {
                start: entry_duration,
                end: Some(entry_duration + center_duration * 0.5),
                from: 0.0,
                to: main_rotate,
                easing: &Easing::EaseInOut,
            },
            KeyFrame {
                start: entry_duration + center_duration * 0.5,
                end: Some(exit_start_time),
                from: main_rotate,
                to: 1.,
                easing: &Easing::EaseInOut,
            },
            KeyFrame {
                start: exit_start_time,
                end: Some(total_duration),
                from: 1.,
                to: rng.gen_range(-5.0..5.0),
                easing: &Easing::EaseIn,
            },
        ]);

        Self {
            photo,
            duration: total_duration,
            rotation_animation,
            transform_animation,
        }
    }
}
