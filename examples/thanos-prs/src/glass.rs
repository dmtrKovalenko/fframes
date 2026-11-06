use std::sync::Arc;

use fframes::media::{ImageData, ImageMetadata, PreloadedImageData};
use fframes::{FFramesContext, FFramesSyncedVideoFrame, Frame};

pub const FOOTAGE: &str = "lava-todo.mp4";
const LEFT: u32 = 56;
const TOP: u32 = 131;
const WIDTH: u32 = 698;
const HEIGHT: u32 = 61;

/// Recover the glass of the first stationary row, including its checkbox, grip and highlights.
/// Solve the source-over equation against the empty margin beside the row. This makes
/// the extracted chrome transparent, so the moving fire and embers still shine through it.
pub fn row(frame: &Frame, ctx: &FFramesContext<'_, '_>) -> Option<ImageData<'static>> {
    let still = frame.video_frame_at(ctx, FOOTAGE, 0.0, None)?.into_image();
    let source = still.href();
    if source.width != 810 || source.height != 720 {
        return None;
    }

    let sample = |x: u32, y: u32, channel: usize| {
        f32::from(source.data[((y * source.width + x) * 4) as usize + channel])
    };
    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in TOP..TOP + HEIGHT {
        let sky: [f32; 3] = std::array::from_fn(|c| {
            (sample(30, y, c) + sample(47, y, c) + sample(774, y, c) + sample(787, y, c)) / 4.0
        });
        // A median across the clear strip rejects isolated embers baked into the still.
        let clear: [f32; 3] = std::array::from_fn(|c| {
            let mut strip: [f32; 17] = std::array::from_fn(|i| sample(360 + 11 * i as u32, y, c));
            strip.sort_by(f32::total_cmp);
            strip[strip.len() / 2]
        });
        for x in LEFT..LEFT + WIDTH {
            let color = if (99..722).contains(&x) {
                clear
            } else {
                std::array::from_fn(|c| sample(x, y, c))
            };
            let alpha = (0..3)
                .map(|c| {
                    let diff = color[c] - sky[c];
                    if diff >= 0.0 {
                        diff / (195.0 - sky[c])
                    } else {
                        -diff / sky[c].max(1.0)
                    }
                })
                .fold(0.0_f32, f32::max)
                .clamp(0.0, 1.0);
            for c in 0..3 {
                let paint = sky[c] + (color[c] - sky[c]) / alpha.max(0.001);
                rgba.push(paint.round().clamp(0.0, 255.0) as u8);
            }
            rgba.push((alpha * 255.0).round() as u8);
        }
    }
    let id = "lava-todo-row".to_string();
    let image = PreloadedImageData::new(id.clone(), WIDTH, HEIGHT, &rgba);
    Some(ImageData::new_from_raw_data(
        Arc::new(image),
        id,
        ImageMetadata {
            width: WIDTH,
            height: HEIGHT,
        },
    ))
}
