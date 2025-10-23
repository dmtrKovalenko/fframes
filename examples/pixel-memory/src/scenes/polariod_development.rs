use crate::{PixelVideo, RandomPhotos};
use fframes::{
    Rotate, Scene, Svgr, Transform, Video,
    animation::{Easing, KeyFrame, KeyFramesAnimation},
    exif::Tag,
    media::ImageData,
};
use rand::Rng;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::OnceLock;

pub struct PolaroidDevelopment<'a> {
    duration: f32,
    photos: Vec<PhotoWithExif<'a>>,
    development_animations: Vec<KeyFramesAnimation<f32>>,
    /// animation for linked x (top-left), y (top-left), rotation
    position_animations: Vec<KeyFramesAnimation<(f32, f32, f32)>>,
    drop_shadow_animations: Vec<KeyFramesAnimation<f32>>,
}

impl Scene for PolaroidDevelopment<'_> {
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
        let video_width = ctx.current_video_size.width as f32;
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
                let (image, year) = photo.image_and_year(ctx)?;

                let original_width = image.metadata.width as f32;
                let original_height = image.metadata.height as f32;

                // Larger photos, thinner border, squarer corners
                const BORDER_SIZE: f32 = 14.0;
                const CAPTION_HEIGHT: f32 = 60.0;
                const MIN_PHOTO_DIMENSION: f32 = 450.0;

                // Dynamic max based on current video size
                let max_photo_width = (video_width * 0.60).max(MIN_PHOTO_DIMENSION);
                let max_photo_height = (video_height * 0.80).max(MIN_PHOTO_DIMENSION);

                let aspect_ratio = original_width / original_height;

                // Fit photo to maxes keeping AR
                let mut photo_width = max_photo_width;
                let mut photo_height = photo_width / aspect_ratio;
                if photo_height > max_photo_height {
                    photo_height = max_photo_height;
                    photo_width = photo_height * aspect_ratio;
                }

                // Enforce minimum visible size
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
                            rx="2"
                            ry="2"
                            fill="#ffffff"
                            stroke="#e9e9e9"
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
                            preserveAspectRatio="xMidYMid slice"
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

#[derive(Debug)]
struct PhotoWithExif<'a> {
    id: &'a str,
    exif_year: OnceLock<Option<String>>,
}

impl<'a, 'media: 'a> PhotoWithExif<'a> {
    fn new(id: &'a str) -> Self {
        Self {
            id,
            exif_year: OnceLock::new(),
        }
    }

    fn image_and_year(
        &'a self,
        ctx: &fframes::FFramesContext<'a, 'media>,
    ) -> Option<(&'media ImageData<'media>, &'a str)> {
        let image = ctx.get_image(self.id)?;
        match self.id {
            "055.jpg" => return Some((image, "1996")),
            "049.jpg" => return Some((image, "2003")),
            "024.jpg" => return Some((image, "2003")),
            "030.jpg" => return Some((image, "2018")),
            "042.jpg" => return Some((image, "2006")),
            "058.jpg" => return Some((image, "2001")),
            "031.jpg" => return Some((image, "2003")),
            "041.jpg" | "060.jpg" => return Some((image, "")),
            _ => (),
        };

        // Parsing EXIF data occurs during the on-demand render, and it’s a rather long, blocking process
        // that only becomes available after the context is created.
        // 1) Cache the result to reuse across frames
        // 2) Avoid performing it in the timeline to speed up the initial load
        let year = match ctx.mode {
            fframes::FFramesMode::EditorTimelinePreview => "",
            _ => self
                .exif_year
                .get_or_init(|| {
                    fframes::log!("Fetching EXIF data for photo: {}", self.id);
                    fframes::log!("{:?}", image.get_exif_data(Tag::DateTimeOriginal));
                    let year = image.get_exif_data(Tag::DateTimeOriginal)?;
                    Some(year.display_value().to_string().get(..4)?.to_string())
                })
                .as_deref()
                .unwrap_or(""),
        };

        Some((image, year))
    }
}

impl<'a> PolaroidDevelopment<'a> {
    pub fn create(photos: &[&'a str], tempo: f32, rng: &mut impl Rng) -> Self {
        const BLOW_OUT_DURATION: f32 = 0.5;
        let photo_count = photos.len();
        let photos = photos.to_vec();

        let base_duration = tempo * 4.0;
        let total_duration = base_duration * photo_count as f32 + (tempo * rng.gen_range(1.0..2.0));

        let mut development_animations = Vec::new();
        let mut position_animations = Vec::new();
        let mut drop_shadow_animations = Vec::new();

        let video_width = PixelVideo::WIDTH as f32;
        let video_height = PixelVideo::HEIGHT as f32;

        // Randomize the effective placement area inside the video ("active area")
        // Slightly smaller than full canvas, with a random offset.
        let active_scale_x = rng.gen_range(0.90..=1.00);
        let active_scale_y = rng.gen_range(0.90..=1.00);
        let active_w = video_width * active_scale_x;
        let active_h = video_height * active_scale_y;
        let active_x0 = rng.gen_range(0.0..=(video_width - active_w).max(0.0));
        let active_y0 = rng.gen_range(0.0..=(video_height - active_h).max(0.0));
        let active_x1 = active_x0 + active_w;
        let active_y1 = active_y0 + active_h;

        // Proxy polaroid size for layout/safe margins (kept large) with slight randomness
        let layout_w = (video_width * rng.gen_range(0.52..=0.60))
            .min(video_height * rng.gen_range(0.62..=0.72));
        let layout_h =
            (video_height * rng.gen_range(0.68..=0.80)).min(layout_w * rng.gen_range(1.12..=1.24));

        // Margin so rotation doesn't clip
        let rotation_margin = (layout_w.max(layout_h) * 0.15).max(40.0);
        let safe_margin_x = layout_w / 2.0 + rotation_margin;
        let safe_margin_y = layout_h / 2.0 + rotation_margin;

        // Compute placement ranges inside the randomized active area, with fallbacks
        let mut x_min = active_x0 + safe_margin_x;
        let mut x_max = active_x1 - safe_margin_x;
        if x_min >= x_max {
            x_min = safe_margin_x;
            x_max = video_width - safe_margin_x;
        }
        if x_min >= x_max {
            // Final fallback: center band
            x_min = video_width * 0.25;
            x_max = video_width * 0.75;
        }

        let mut y_min = active_y0 + safe_margin_y;
        let mut y_max = active_y1 - safe_margin_y;
        if y_min >= y_max {
            y_min = safe_margin_y;
            y_max = video_height - safe_margin_y;
        }
        if y_min >= y_max {
            // Final fallback: center band
            y_min = video_height * 0.25;
            y_max = video_height * 0.75;
        }

        // Blue-noise-ish spread: keep minimum distance between centers to avoid bunching,
        // but small enough to allow overlap (not full cover).
        let min_dist = 0.40 * layout_w.max(layout_h); // allows partial overlap, prevents full cover
        let min_dist_sq = min_dist * min_dist;

        let mut centers: Vec<(f32, f32)> = Vec::with_capacity(photo_count);

        for index in 0..photo_count {
            let start_time = index as f32 * tempo * 3.0;

            development_animations.push(KeyFramesAnimation::new(vec![KeyFrame {
                start: start_time,
                end: Some(start_time + tempo * 2.0),
                from: 0.0,
                to: 1.0,
                easing: &Easing::EaseOut,
            }]));

            // Rejection sampling to place centers with min distance constraint within randomized area
            let (cx, cy) = {
                let mut cx;
                let mut cy;
                let mut attempts = 0;
                let mut local_min_dist_sq = min_dist_sq;

                loop {
                    attempts += 1;

                    cx = rng.gen_range(x_min..x_max);
                    cy = rng.gen_range(y_min..y_max);

                    // Check distance from previous centers
                    let ok = centers.iter().all(|&(px, py)| {
                        let dx = cx - px;
                        let dy = cy - py;
                        let d2 = dx * dx + dy * dy;
                        d2 >= local_min_dist_sq
                    });

                    if ok {
                        break (cx, cy);
                    }

                    // After many attempts, gradually relax the distance so we always find a spot
                    if attempts > 48 {
                        local_min_dist_sq *= 0.85; // relax 15%
                        attempts = 0;
                    }
                }
            };

            centers.push((cx, cy));

            let target_x = cx - layout_w / 2.0;
            let target_y = cy - layout_h / 2.0;

            let rotation = rng.gen_range(-4.0..4.0);
            let last_position = (
                target_x + rng.gen_range(-10.0..10.0),
                target_y + rng.gen_range(-10.0..10.0),
                rotation + rng.gen_range(-1.0..1.0),
            );

            position_animations.push(KeyFramesAnimation::new(vec![
                KeyFrame {
                    start: start_time - 0.5,
                    end: Some(start_time + 0.5),
                    from: (target_x, -layout_h, 0.0), // drop from above
                    to: (target_x, target_y, rotation),
                    easing: &Easing::EaseOut,
                },
                KeyFrame {
                    start: start_time + 0.5,
                    end: Some(total_duration),
                    from: (target_x, target_y, rotation),
                    to: last_position,
                    easing: &Easing::EaseInOut,
                },
                KeyFrame {
                    start: total_duration,
                    end: Some(total_duration + BLOW_OUT_DURATION),
                    from: last_position,
                    to: (
                        if index % 2 == 0 { -2000.0 } else { 2000.0 },
                        if (index + index / 2) % 2 == 0 {
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
            photos: photos.into_iter().map(PhotoWithExif::new).collect(),
            development_animations,
            position_animations,
            drop_shadow_animations,
        }
    }

    pub fn generate(rng: &mut impl Rng, tempo: f32, images: &mut RandomPhotos<'a>) -> Self {
        let photo_count = rng.gen_range(4..=8);
        let photos = images.choose(photo_count);
        Self::create(&photos, tempo, rng)
    }
}

impl Debug for PolaroidDevelopment<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let photo_names: Vec<_> = self.photos.iter().map(|p| p.id).collect();
        write!(f, "PolaroidDevelopment: {}", photo_names.join(", "))
    }
}
