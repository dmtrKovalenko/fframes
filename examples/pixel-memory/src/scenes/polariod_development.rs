use crate::{PixelVideo, RandomPhotos};
use fframes::{
    Rotate, Scene, Svgr, Transform, Video,
    animation::{Easing, KeyFrame, KeyFramesAnimation},
    exif,
};
use rand::Rng;

#[derive(Debug)]
pub struct PolaroidDevelopment<'a> {
    duration: f32,
    photos: Vec<&'a str>,
    development_animations: Vec<KeyFramesAnimation<f32>>,
    /// animation for linked x, y, rotation
    position_animations: Vec<KeyFramesAnimation<(f32, f32, f32)>>,
    drop_shadow_animations: Vec<KeyFramesAnimation<f32>>,
}

impl Scene for PolaroidDevelopment<'_> {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(self.duration)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        let video_height = ctx.current_video_size.height as f32;

        fframes::svgr!(
            <defs>
                <filter id="polaroid-shadow" x="-20%" y="-20%" width="140%" height="140%">
                    <feGaussianBlur in="SourceAlpha" stdDeviation="10" />
                    <feOffset dx="5" dy="5" result="offsetblur" />
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
                let year = image.exif_data.as_ref().and_then(|exif_| {
                    exif_.fields.iter().find(|entry| entry.tag == exif::Tag::DateTime)
                }).map(|entry| {
                    let datetime_str = entry.display_value().to_string();
                    datetime_str.get(0..4).unwrap_or("").to_string()
                }).unwrap_or("".to_string());

                let original_width = image.metadata.width as f32;
                let original_height = image.metadata.height as f32;

                const BORDER_SIZE: f32 = 30.0;
                const CAPTION_HEIGHT: f32 = 70.0;
                const MIN_PHOTO_DIMENSION: f32 = 250.0;
                const MAX_PHOTO_WIDTH: f32 = 650.0;

                let aspect_ratio = original_width / original_height;

                let mut photo_width = MAX_PHOTO_WIDTH;
                let mut photo_height = photo_width / aspect_ratio;

                let max_height = video_height * 0.5;
                if photo_height > max_height {
                    photo_height = max_height;
                    photo_width = photo_height * aspect_ratio;
                }

                if photo_width < MIN_PHOTO_DIMENSION && photo_width < photo_height {
                    photo_width = MIN_PHOTO_DIMENSION;
                    photo_height = photo_width / aspect_ratio;
                } else if photo_height < MIN_PHOTO_DIMENSION && photo_height < photo_width {
                    photo_height = MIN_PHOTO_DIMENSION;
                    photo_width = photo_height * aspect_ratio;
                }

                let polaroid_width = photo_width + (BORDER_SIZE * 2.0);
                let polaroid_height = photo_height + BORDER_SIZE + CAPTION_HEIGHT;

                let (translate_x, translate_y, rotation) = frame.animate(&self.position_animations[index]);
                let shadow_opacity = frame.animate(&self.drop_shadow_animations[index]);
                let development_progress = frame.animate(&self.development_animations[index]);

                let center_x = polaroid_width / 2.0;
                let center_y = polaroid_height / 2.0;

                Some(fframes::svgr!(
                    <g
                        transform={Transform {
                            translate_x: translate_x.into(),
                            translate_y: translate_y.into(),
                            rotate: Rotate {
                                angle: rotation as f64,
                                 origin: Some((center_x as f64, center_y as f64))
                            },
                            ..Default::default()
                        }}
                        filter="url(#polaroid-shadow)"
                        opacity={shadow_opacity}
                    >
                        <rect
                            width={polaroid_width}
                            height={polaroid_height}
                            rx="3"
                            ry="3"
                            fill="#ffffff"
                            stroke="#eeeeee"
                            stroke-width="1"
                        />

                        <rect
                            x={BORDER_SIZE}
                            y={BORDER_SIZE}
                            width={photo_width}
                            height={photo_height}
                            fill="#f0f0f0"
                        />

                        <image
                            href={image.href()}
                            x={BORDER_SIZE}
                            y={BORDER_SIZE}
                            width={photo_width}
                            height={photo_height}
                        />

                        // White overlay that fades out during "development"
                        <rect
                            x={BORDER_SIZE}
                            y={BORDER_SIZE}
                            width={photo_width}
                            height={photo_height}
                            fill="#ffffff"
                            opacity={1.0 - development_progress}
                        />

                        <text
                            x={polaroid_width / 2.0}
                            y={polaroid_height - CAPTION_HEIGHT / 2.0 + 5.0}
                            text-anchor="middle"
                            dominant-baseline="middle"
                            font-family="'Indie Flower', cursive, sans-serif"
                            font-size="44"
                            fill="#333333"
                            opacity={development_progress}
                        >
                            {year}
                        </text>
                    </g>
                ))
            }).collect::<Svgr>()}
        )
    }
}

impl<'a> PolaroidDevelopment<'a> {
    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        const BLOW_OUT_DURATION: f32 = 0.5;
        let photo_count = rng.gen_range(4..=8);
        let photos = images.choose(photo_count);

        let base_duration = tempo * 4.0;
        let total_duration = base_duration * photo_count as f32 + (tempo * rng.gen_range(1.0..2.0));

        let mut development_animations = Vec::new();
        let mut position_animations = Vec::new();
        let mut drop_shadow_animations = Vec::new();

        let video_width = PixelVideo::WIDTH as f32;
        let video_height = PixelVideo::HEIGHT as f32;

        for (index, _) in photos.iter().enumerate() {
            let polaroid_width: f32 = 450.0 + rng.gen_range(60.0..80.0);
            let polaroid_height: f32 = 450.0 + rng.gen_range(80.0..120.0);

            let start_time = index as f32 * tempo * 3.0;
            development_animations.push(KeyFramesAnimation::new(vec![KeyFrame {
                start: start_time,
                end: Some(start_time + tempo * 2.0),
                from: 0.0,
                to: 1.0,
                easing: &Easing::EaseOut,
            }]));

            // Calculate safe boundaries to ensure polaroid stays fully in frame
            // Account for rotation by adding extra margin
            let rotation_margin = (polaroid_width.max(polaroid_height) * 0.2).max(50.0);

            let safe_margin_x = polaroid_width / 2.0 + rotation_margin;
            let safe_margin_y = polaroid_height / 2.0 + rotation_margin;

            let target_x = rng.gen_range(safe_margin_x..video_width - safe_margin_x);
            let target_y = rng.gen_range(safe_margin_y..video_height - safe_margin_y);

            let rotation = rng.gen_range(-5.0..5.0);
            let last_position = (
                target_x - polaroid_width / 2.0,
                target_y - polaroid_height / 2.0 - rng.gen_range(5.0..10.0),
                rotation + rng.gen_range(-1.0..1.0),
            );

            position_animations.push(KeyFramesAnimation::new(vec![
                KeyFrame {
                    start: start_time - 0.5,
                    end: Some(start_time + 0.5),
                    from: (target_x - polaroid_width / 2.0, -polaroid_height, 0.0),
                    to: (
                        target_x - polaroid_width / 2.0,
                        target_y - polaroid_height / 2.0,
                        rotation,
                    ),
                    easing: &Easing::EaseOut,
                },
                // Add a subtle floating motion
                KeyFrame {
                    start: start_time + 0.5,
                    end: Some(total_duration),
                    from: (
                        target_x - polaroid_width / 2.0,
                        target_y - polaroid_height / 2.0,
                        rotation,
                    ),
                    to: last_position,
                    easing: &Easing::EaseInOut,
                },
                KeyFrame {
                    start: total_duration,
                    end: Some(total_duration + BLOW_OUT_DURATION),
                    from: last_position,
                    to: (
                        if index % 2 == 0 { -2000.0 } else { 2000.0 },
                        if index + index / 2 % 2 == 0 {
                            -2000.0
                        } else {
                            2000.0
                        },
                        last_position.2,
                    ),
                    easing: &Easing::EaseIn,
                },
            ]));

            drop_shadow_animations.push(KeyFramesAnimation::new(vec![KeyFrame {
                start: start_time - 0.5,
                end: Some(start_time + 0.5),
                from: 0.0,
                to: 1.0,
                easing: &Easing::EaseOut,
            }]));
        }

        Self {
            duration: total_duration + BLOW_OUT_DURATION,
            photos,
            development_animations,
            position_animations,
            drop_shadow_animations,
        }
    }
}
