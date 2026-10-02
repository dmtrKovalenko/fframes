use crate::{PhotoFrame, RandomPhotos, photo_phrame::FramedImage};
use fframes::{
    Scene, Svgr, Transform,
    animation::{Easing, KeyFrame, KeyFramesAnimation},
};
use rand::Rng;
use std::f32::consts::PI;
use std::fmt::{Debug, Formatter, Result as FmtResult};

const GOLDEN_RATIO: f32 = 1.618_034; // Golden ratio (φ)
const BASE_PHOTO_SIZE: f32 = 1000.0; // Base size for photos in pixels
const SPIRAL_BASE_RADIUS: f32 = 60.0; // Starting radius for spiral
const SPIRAL_ANGLE_INCREMENT: f32 = 0.5; // Angle increment in PI units
const EDGE_OFFSET: f32 = 200.0; // How far off-screen photos start

pub struct SpiralHeapGallery<'a> {
    duration: f32,
    photos: Vec<&'a str>,
    position_scale_animations: Vec<KeyFramesAnimation<(f32, f32, f32)>>, // x, y, scale combined
    rotation_animations: Vec<KeyFramesAnimation<f32>>,
    opacity_animations: Vec<KeyFramesAnimation<f32>>,
}

impl Scene for SpiralHeapGallery<'_> {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Next(0.5)
    }

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(self.duration)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        fframes::svgr!(
            <defs>
                // Drop shadow for photos
                <filter id="photo-shadow" x="-20%" y="-20%" width="140%" height="140%">
                    <feGaussianBlur in="SourceAlpha" stdDeviation="8" />
                    <feOffset dx="3" dy="3" result="offsetblur" />
                    <feComponentTransfer>
                        <feFuncA type="linear" slope="0.3" />
                    </feComponentTransfer>
                    <feMerge>
                        <feMergeNode />
                        <feMergeNode in="SourceGraphic" />
                    </feMerge>
                </filter>
            </defs>

            {self.photos.iter().enumerate().filter_map(|(index, photo)| {
                let image = ctx.get_image(photo)?;

                let (x, y, scale) = frame.animate(&self.position_scale_animations[index]);
                let rotation = frame.animate(&self.rotation_animations[index]);
                let opacity = frame.animate(&self.opacity_animations[index]);

                let original_width = image.metadata.width as f32;
                let original_height = image.metadata.height as f32;
                let aspect_ratio = original_width / original_height;

                let photo_width = if aspect_ratio > 1.0 {
                    BASE_PHOTO_SIZE
                } else {
                    BASE_PHOTO_SIZE * aspect_ratio
                };

                let photo_height = if aspect_ratio > 1.0 {
                    BASE_PHOTO_SIZE / aspect_ratio
                } else {
                    BASE_PHOTO_SIZE
                };

                Some(fframes::svgr!(
                    <g
                        transform={
                            Transform {
                                translate_x: x.into(),
                                translate_y: y.into(),
                                rotate: rotation.into(),
                                scale: scale.into(),
                                ..Default::default()
                            }
                        }
                        filter="url(#photo-shadow)"
                        opacity={opacity}
                    >
                        {image.render_framed(PhotoFrame {
                            x: -photo_width / 2.0,
                            y: -photo_height / 2.0,
                            width: photo_width as u32,
                            height: photo_height as u32,
                            ..Default::default()
                        })}
                    </g>
                ))
            }).collect::<Svgr>()}
        )
    }
}

impl<'a> SpiralHeapGallery<'a> {
    pub fn create(photos: &[&'a str], tempo: f32, rng: &mut impl Rng) -> Self {
        let photo_count = photos.len();
        let photos = photos.to_vec();

        let last_photo_animation_end = ((photo_count - 1) as f32 * tempo * 4.0) + tempo * 4.0;
        let display_duration = rng.gen_range(5.0..8.0);
        let total_duration = last_photo_animation_end + display_duration;

        let mut position_scale_animations = Vec::new();
        let mut rotation_animations = Vec::new();
        let mut opacity_animations = Vec::new();

        let video_width = 1920.0;
        let video_height = 1080.0;
        let center_x = video_width / 2.0;
        let center_y = video_height / 2.0;

        let mut final_positions = Vec::new();
        for i in 0..photo_count {
            let theta = i as f32 * SPIRAL_ANGLE_INCREMENT * PI;
            let radius = SPIRAL_BASE_RADIUS * GOLDEN_RATIO.powf(theta / PI);

            let mut final_x = center_x + radius * theta.cos();
            let mut final_y = center_y + radius * theta.sin();

            // Calculate max photo dimensions to ensure they stay within screen bounds
            let max_photo_dimension = BASE_PHOTO_SIZE / 2.0; // Half size since photos are centered
            let margin = 50.0; // Additional margin from screen edge

            // Clamp positions to keep photos within screen boundaries
            final_x = final_x
                .max(max_photo_dimension + margin)
                .min(video_width - max_photo_dimension - margin);
            final_y = final_y
                .max(max_photo_dimension + margin)
                .min(video_height - max_photo_dimension - margin);

            final_positions.push((final_x, final_y, theta, radius));
        }

        for (i, (final_x, final_y, theta, _)) in final_positions.iter().copied().enumerate() {
            let start_time = i as f32 * tempo * 4.0;
            let animation_duration = tempo * 4.0;
            let angle_normalized = (theta % (2.0 * PI)) / (2.0 * PI);

            let (start_x, start_y) = if angle_normalized < 0.25 {
                (
                    center_x + rng.gen_range(-video_width / 2.0..video_width / 2.0),
                    -EDGE_OFFSET,
                )
            } else if angle_normalized < 0.5 {
                (
                    video_width + EDGE_OFFSET,
                    center_y + rng.gen_range(-video_height / 2.0..video_height / 2.0),
                )
            } else if angle_normalized < 0.75 {
                (
                    center_x + rng.gen_range(-video_width / 2.0..video_width / 2.0),
                    video_height + EDGE_OFFSET,
                )
            } else {
                (
                    -EDGE_OFFSET,
                    center_y + rng.gen_range(-video_height / 2.0..video_height / 2.0),
                )
            };

            let spring = Easing::Spring {
                stiffness: rng.gen_range(30.0..45.0),
                mass: rng.gen_range(4.0..5.0),
                damping: rng.gen_range(20.0..35.0),
            };

            let final_position = (
                final_x + rng.gen_range(-10.0..10.0),
                final_y + rng.gen_range(-10.0..10.0),
                1.0 + rng.gen_range(0.0..0.02),
            );
            position_scale_animations.push(KeyFramesAnimation::new(vec![
                KeyFrame {
                    start: start_time,
                    end: Some(start_time + animation_duration),
                    from: (start_x, start_y, 0.1),
                    to: (final_x, final_y, 1.0),
                    easing: &spring,
                },
                KeyFrame {
                    start: start_time + animation_duration,
                    end: Some(total_duration - 0.5),
                    from: (final_x, final_y, 1.0),
                    to: final_position,
                    easing: &Easing::EaseInOut,
                },
                KeyFrame {
                    start: total_duration - 0.5,
                    end: Some(total_duration),
                    from: final_position,
                    to: (
                        if rng.gen_bool(0.5) { 2000.0 } else { -2000.0 },
                        if rng.gen_bool(0.5) { 2000.0 } else { -2000.0 },
                        rng.gen_range(0.1..0.8),
                    ),
                    easing: &Easing::EaseInOut,
                },
            ]));

            let initial_rotation = rng.gen_range(-20.0..20.0);
            let final_rotation = rng.gen_range(-5.0..5.0);

            rotation_animations.push(KeyFramesAnimation::new(vec![
                KeyFrame {
                    start: start_time,
                    end: Some(start_time + animation_duration),
                    from: initial_rotation,
                    to: final_rotation,
                    easing: &Easing::EaseOut,
                },
                KeyFrame {
                    start: start_time + animation_duration,
                    end: Some(total_duration - 0.5),
                    from: final_rotation,
                    to: final_rotation + rng.gen_range(-2.0..2.0),
                    easing: &Easing::EaseInOut,
                },
                KeyFrame {
                    start: total_duration - 0.5,
                    end: Some(total_duration),
                    from: final_rotation + rng.gen_range(-2.0..2.0),
                    to: final_rotation + rng.gen_range(30.0..60.0),
                    easing: &Easing::EaseIn,
                },
            ]));

            opacity_animations.push(KeyFramesAnimation::new(vec![
                KeyFrame {
                    start: start_time,
                    end: Some(start_time + tempo * 0.8),
                    from: 0.0,
                    to: 1.0,
                    easing: &Easing::EaseOut,
                },
                KeyFrame {
                    start: total_duration - 0.5,
                    end: Some(total_duration),
                    from: 1.0,
                    to: 0.0,
                    easing: &Easing::EaseIn,
                },
            ]));
        }

        Self {
            duration: total_duration,
            photos,
            position_scale_animations,
            rotation_animations,
            opacity_animations,
        }
    }

    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let photo_count = rng.gen_range(6..=10);
        let photos = images.choose(photo_count);
        Self::create(&photos, tempo, rng)
    }
}

impl Debug for SpiralHeapGallery<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "SpiralHeapGallery: {}", self.photos.join(", "))
    }
}
