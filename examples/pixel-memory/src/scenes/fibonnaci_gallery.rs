use crate::photo_phrame::PhotoFrame;
use crate::pixel_video_randomizer::RandomPhotos;
use crate::{FramedImage, PixelVideo};
use fframes::{Scene, Svgr, Transform, Video};
use rand::Rng;
use std::f32::consts::PI;

/// The golden ratio (φ = (1 + √5) / 2)
const PHI: f32 = 1.618_034;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::atomic::AtomicBool;

pub struct FibonacciSpiralGallery<'a> {
    duration: f32,
    photo: &'a str,
    base_photo_size: f32,
    fade_in_duration: f32,
    center_duration: f32,
    spiral_out_duration: f32,
    spiral_direction: f32,
    max_spiral_radius: f32,
    spiral_revolutions: f32,
}

impl Scene for FibonacciSpiralGallery<'_> {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Next(self.spiral_out_duration * 0.3)
    }

    fn duration(&self) -> fframes::Duration<'_> {
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

        let original_width = image.metadata.width as f32;
        let original_height = image.metadata.height as f32;
        let aspect_ratio = original_width / original_height;

        let photo_width = if aspect_ratio > 1.0 {
            self.base_photo_size
        } else {
            self.base_photo_size * aspect_ratio
        };

        let photo_height = if aspect_ratio > 1.0 {
            self.base_photo_size / aspect_ratio
        } else {
            self.base_photo_size
        };

        let mut opacity = 1.0;
        let transform = if current_time < self.fade_in_duration {
            let phase_progress = current_time / self.fade_in_duration;
            opacity = phase_progress;

            Transform {
                translate_x: f64::from(center_x),
                translate_y: f64::from(center_y),
                scale: (0.7 + 0.3 * f64::from(phase_progress)).into(),
                ..Default::default()
            }
        } else if current_time < (self.fade_in_duration + self.center_duration) {
            Transform {
                translate_x: f64::from(center_x),
                translate_y: f64::from(center_y),
                scale: 1.0.into(),
                ..Default::default()
            }
        } else {
            let phase_elapsed = current_time - self.fade_in_duration - self.center_duration;
            let phase_progress = phase_elapsed / self.spiral_out_duration;

            let growth_factor = phase_progress.powf(0.8);
            opacity = 1.0 - phase_progress.powf(0.7);
            let angle_progress = phase_progress.powf(0.6);

            // this is the position of the spiral (θ) in radians
            let theta = self.spiral_direction * angle_progress * self.spiral_revolutions * 2.0 * PI;

            // The golden ratio growth factor b = ln(φ)/(2π)
            let b = PHI.ln() / (2.0 * PI);
            let base_radius = (b * theta.abs()).exp();
            let max_radius_factor = (b * self.spiral_revolutions * 2.0 * PI).exp();

            // Logarithmic Spiral Formula:
            // r = a * e^(b*θ)
            // where b is a the growth factor for the spiral movement
            let spiral_radius =
                self.max_spiral_radius * base_radius / max_radius_factor * growth_factor;

            let start_blend = phase_progress.powf(0.3);
            let radius = 0.1 * (1.0 - start_blend) + spiral_radius * start_blend;

            // Convert to cartesian coordinates
            let spiral_x = center_x + radius * theta.cos();
            let spiral_y = center_y + radius * theta.sin();

            Transform {
                translate_x: f64::from(spiral_x),
                translate_y: f64::from(spiral_y),
                scale: (1.0 - 0.9 * f64::from(phase_progress)).into(),
                rotate: (f64::from(self.spiral_direction) * f64::from(phase_progress) * 25.0)
                    .into(),
                ..Default::default()
            }
        };

        fframes::svgr!(
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

            <g
                transform={transform}
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
    }
}

static DIRECTION: AtomicBool = AtomicBool::new(false);

impl<'a> FibonacciSpiralGallery<'a> {
    pub fn create(photo: &'a str, tempo: f32, rng: &mut impl Rng) -> Self {
        let base_photo_size = rng.gen_range(1200..1400) as f32;

        let fade_in_duration = tempo * rng.gen_range(1..2) as f32;
        let center_duration = tempo * rng.gen_range(2..3) as f32;

        let _ = rng.gen_bool(0.5); // Dummy call to avoid unused variable warning

        let is_left = DIRECTION.load(std::sync::atomic::Ordering::Relaxed);
        let spiral_direction = if is_left { 1.0 } else { -1.0 };
        DIRECTION.store(!is_left, std::sync::atomic::Ordering::Relaxed);

        let spiral_revolutions = rng.gen_range(1.0..3.0);
        let spiral_out_duration = tempo * spiral_revolutions * PI;

        let max_spiral_radius = rng.gen_range(350.0..650.0);
        let total_duration = fade_in_duration + center_duration + spiral_out_duration;
        Self {
            duration: total_duration,
            photo,
            base_photo_size,
            fade_in_duration,
            center_duration,
            spiral_out_duration,
            spiral_direction,
            max_spiral_radius,
            spiral_revolutions,
        }
    }

    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let photo = images.choose_one();
        Self::create(photo, tempo, rng)
    }
}

impl Debug for FibonacciSpiralGallery<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "FibonacciSpiralGallery: {}", self.photo)
    }
}
