use crate::photo_phrame::PhotoFrame;
use crate::pixel_video_randomizer::RandomPhotos;
use crate::{FramedImage, PixelVideo};
use fframes::{Scene, Svgr, Video};
use rand::Rng;
use std::f32::consts::PI;

const BASE_PHOTO_SIZE: f32 = 950.0; // Base size for photos in pixels
const SPIRAL_REVOLUTIONS: f32 = 1.2; // Just over 1 revolution for exit

#[derive(Debug)]
pub struct FibonacciSpiralGallery<'a> {
    duration: f32,
    photo: &'a str,
    photo_animation: PhotoAnimation,
}

// Store animation parameters for the photo
#[derive(Debug)]
struct PhotoAnimation {
    fade_in_duration: f32,
    center_duration: f32,
    spiral_out_duration: f32,
    spiral_angle: f32,
    spiral_direction: f32,
    max_spiral_radius: f32,
}

impl Scene for FibonacciSpiralGallery<'_> {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(self.duration)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        let center_x = PixelVideo::WIDTH as f32 / 2.0;
        let center_y = PixelVideo::HEIGHT as f32 / 2.0;
        let current_time = frame.seconds();

        let Some(image) = ctx.get_image(self.photo) else {
            return Svgr::empty();
        };

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

            {
                let animation = &self.photo_animation;
                let total_animation_duration = animation.fade_in_duration + animation.center_duration + animation.spiral_out_duration;

                if current_time <= total_animation_duration {
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

                    // Determine which phase of animation we're in
                    let (x, y, scale, opacity, rotation) = if current_time < animation.fade_in_duration {
                        // Phase 1: Fade in at center (no spiral)
                        let phase_progress = current_time / animation.fade_in_duration;

                        (
                            center_x,
                            center_y,
                            0.7 + 0.3 * phase_progress, // Scale from 0.7 to 1.0
                            phase_progress, // Opacity from 0.0 to 1.0
                            0.0 // No rotation
                        )
                    } else if current_time < (animation.fade_in_duration + animation.center_duration) {
                        (
                            center_x,
                            center_y,
                            1.0, // Full scale
                            1.0, // Full opacity
                            0.0  // No rotation
                        )
                    } else {
                        // Phase 3: Spiral out
                        let phase_elapsed = current_time - animation.fade_in_duration - animation.center_duration;
                        let phase_progress = phase_elapsed / animation.spiral_out_duration;

                        // Calculate spiral position (from center to outside)
                        let angle = animation.spiral_angle + animation.spiral_direction * phase_progress * SPIRAL_REVOLUTIONS * 2.0 * PI;
                        let radius = animation.max_spiral_radius * phase_progress;

                        let spiral_x = center_x + radius * angle.cos();
                        let spiral_y = center_y + radius * angle.sin();

                        let scale_factor = 1.0 - 0.7 * phase_progress; // Scale from 1.0 to 0.3
                        let opacity_factor = 1.0 - phase_progress.powf(0.7);
                        let rotation_factor = animation.spiral_direction * phase_progress * 25.0; // Gentle rotation

                        (spiral_x, spiral_y, scale_factor, opacity_factor, rotation_factor)
                    };

                    fframes::svgr!(
                        <g
                            transform={format!("translate({}, {}) rotate({}) scale({})",
                                x, y, rotation, scale)}
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
                    )
                } else {
                    Svgr::empty()
                }
            }
        )
    }
}

impl<'a> FibonacciSpiralGallery<'a> {
    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let photo = images.choose_one();

        let fade_in_duration = tempo * rng.gen_range(1.0..1.5); // Simple fade in
        let center_duration = tempo * rng.gen_range(2.0..3.0); // Time at center
        let spiral_out_duration = tempo * rng.gen_range(2.5..3.5); // Spiral out duration

        let spiral_angle = rng.gen_range(0.0..2.0 * PI);
        let spiral_direction = if rng.gen_bool(0.5) { 1.0 } else { -1.0 };

        let max_spiral_radius = rng.gen_range(250.0..350.0);
        let photo_animation = PhotoAnimation {
            fade_in_duration,
            center_duration,
            spiral_out_duration,
            spiral_angle,
            spiral_direction,
            max_spiral_radius,
        };

        // Calculate total duration
        let total_duration = fade_in_duration + center_duration + spiral_out_duration + 0.5;

        Self {
            duration: total_duration,
            photo,
            photo_animation,
        }
    }
}
