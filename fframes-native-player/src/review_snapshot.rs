//! Review snapshots of the control bar over a real frame, for looking at the bar without a
//! window. Not part of the test suite:
//!
//! `FFRAMES_PLAYER_FRAME=frame.png FFRAMES_PLAYER_OUT=out.png cargo test --release \
//!   -p fframes_native_player review_snapshot -- --ignored`
//!
//! Optional: `FFRAMES_PLAYER_SIZE=2560x1440` (physical px), `FFRAMES_PLAYER_SCALE=2`,
//! `FFRAMES_PLAYER_AT=frame`, `FFRAMES_PLAYER_PLAYING=1`, `FFRAMES_PLAYER_HOVER=play`.

use fframes_skia_renderer::skia_safe::{
    Color, Data, EncodedImageFormat, Image, Paint, Rect, surfaces,
};

use crate::controls::{Control, Controls, ControlsLayout, ControlsState, format_time};

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "writes a review image, run by hand with the FFRAMES_PLAYER_* variables"]
fn review_snapshot() {
    let frame_path = std::env::var("FFRAMES_PLAYER_FRAME").expect("FFRAMES_PLAYER_FRAME");
    let out = std::env::var("FFRAMES_PLAYER_OUT").expect("FFRAMES_PLAYER_OUT");
    let size = std::env::var("FFRAMES_PLAYER_SIZE").unwrap_or_else(|_| "2560x1440".to_owned());
    let (width, height) = size
        .split_once('x')
        .map(|(w, h)| (w.parse::<i32>().unwrap(), h.parse::<i32>().unwrap()))
        .expect("WxH");
    let scale: f32 = env_or("FFRAMES_PLAYER_SCALE", 2.);
    let fps = 60;
    let duration_in_frames = 60 * 32;
    let frame: usize = env_or("FFRAMES_PLAYER_AT", 150);
    let playing = env_or("FFRAMES_PLAYER_PLAYING", 0) == 1;
    let hovered = match std::env::var("FFRAMES_PLAYER_HOVER").as_deref() {
        Ok("play") => Some(Control::PlayPause),
        Ok("seek") => Some(Control::Seek),
        Ok("loop") => Some(Control::Loop),
        Ok("back") => Some(Control::Back),
        _ => None,
    };

    let bytes = std::fs::read(&frame_path).expect("frame png");
    let image = Image::from_encoded(Data::new_copy(&bytes)).expect("decodable png");

    let mut surface = surfaces::raster_n32_premul((width, height)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(Color::BLACK);

    let s = (width as f32 / image.width() as f32).min(height as f32 / image.height() as f32);
    let (w, h) = (image.width() as f32 * s, image.height() as f32 * s);
    let dst = Rect::from_xywh((width as f32 - w) / 2., (height as f32 - h) / 2., w, h);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    canvas.draw_image_rect(&image, None, dst, &paint);

    let controls = Controls::new();
    let layout = ControlsLayout::new(
        &controls,
        &format_time(duration_in_frames, fps),
        width as f32,
        height as f32,
        scale,
    );
    let state = ControlsState {
        visible: true,
        frame,
        duration_in_frames,
        fps,
        playing,
        looping: true,
        fullscreen: false,
        hovered,
        pointer_x: std::env::var("FFRAMES_PLAYER_POINTER_X")
            .ok()
            .and_then(|x| x.parse().ok()),
    };
    controls.draw(canvas, &layout, &state);

    let png = surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, 100)
        .expect("png");
    std::fs::write(&out, png.as_bytes()).expect("write png");
}
