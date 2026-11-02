//! A review film of the player's chrome: a scripted pointer uses the studio and the focus
//! view over real footage and every frame is written as a PNG, to watch the chrome move
//! without a window. Not part of the test suite:
//!
//! `FFRAMES_PLAYER_FOOTAGE=frames FFRAMES_PLAYER_OUT=film cargo test --release \
//!   -p fframes_native_player review_film -- --ignored`
//!
//! `frames` holds `f_00000.png`, `f_00001.png`, ... at 60 fps; the film is 60 fps too.
//! Optional: `FFRAMES_PLAYER_SIZE=1280x720` (logical px), `FFRAMES_PLAYER_SCALE=2`,
//! `FFRAMES_PLAYER_MOTION=0` cuts every motion (how the chrome drew before it moved).
//! Then `ffmpeg -framerate 60 -i film/%05d.png -pix_fmt yuv420p film.mp4`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use fframes::usvgr;
use fframes_skia_renderer::skia_safe::{
    Canvas, Color, Data, EncodedImageFormat, FilterMode, Image, Paint, PaintJoin, PaintStyle,
    PathBuilder, Point, Rect, SamplingOptions, surfaces,
};

use crate::controls::{Control, Controls, ControlsLayout, ControlsState, format_time, mix};
use crate::motion::{Easing, Intent, Motion, Timing};
use crate::studio::{Studio, StudioLayout, StudioScroll};

const FPS: usize = 60;
const IDLE_TIMEOUT: f32 = 2.5;
const LENGTH: f32 = 18.;
const THUMBNAILS: usize = 14;

/// What the pointer does, in logical pixels and seconds.
enum Event {
    Enter(Point),
    /// A travel to a target that ends at this time.
    MoveTo(Target, f32),
    Press,
    Release,
    Space,
}

enum Target {
    /// A button of the studio transport or the focus dock.
    Button(Control),
    /// The scene band at a frame.
    Scene(usize),
    /// The film strip at a frame.
    Strip(usize),
    /// The "Studio · T" pill of the focus view.
    Restore,
    Point(Point),
}

fn script() -> Vec<(f32, Event)> {
    vec![
        (1.0, Event::Enter(Point::new(640., 250.))),
        (1.0, Event::MoveTo(Target::Scene(420), 2.2)),
        (2.6, Event::Press),
        (2.72, Event::Release),
        (3.2, Event::MoveTo(Target::Strip(250), 4.0)),
        (4.3, Event::Press),
        (4.3, Event::MoveTo(Target::Strip(460), 5.3)),
        (5.5, Event::Release),
        (6.0, Event::MoveTo(Target::Button(Control::PlayPause), 6.8)),
        (7.1, Event::Press),
        (7.22, Event::Release),
        (7.6, Event::MoveTo(Target::Button(Control::Focus), 8.4)),
        (8.7, Event::Press),
        (8.82, Event::Release),
        (9.2, Event::MoveTo(Target::Button(Control::Loop), 10.0)),
        (10.3, Event::Press),
        (10.42, Event::Release),
        (
            10.8,
            Event::MoveTo(Target::Point(Point::new(640., 300.)), 11.6),
        ),
        (14.6, Event::Space),
        (15.4, Event::MoveTo(Target::Restore, 16.2)),
        (16.5, Event::Press),
        (16.62, Event::Release),
    ]
}

/// The player's state the chrome reacts to, mirroring `app.rs`.
struct Player {
    playing: bool,
    looping: bool,
    studio_open: bool,
    /// The playhead, in frames of the footage.
    position: f64,
    last_frame: usize,
    pointer: Option<Point>,
    hovered: Option<Control>,
    pressed: Option<Control>,
    drag: Option<Control>,
    last_activity: f32,
    controls_visible: bool,
    travel: Option<(Point, Point, f32, f32)>,
}

impl Player {
    fn controls_should_be_visible(&self, t: f32) -> bool {
        !self.playing
            || self.drag.is_some()
            || self.hovered.is_some()
            || t - self.last_activity < IDLE_TIMEOUT
    }

    fn control_under_cursor(
        &self,
        studio: &StudioLayout,
        model: &Studio,
        dock: &ControlsLayout,
        scale: f32,
    ) -> Option<Control> {
        let point = self.pointer?;
        if self.studio_open {
            return studio.hit(point, model);
        }
        if self.controls_visible
            && let Some(control) = dock.hit(point.x * scale, point.y * scale)
        {
            return Some(control);
        }
        (self.controls_visible && studio.focus_hit(point)).then_some(Control::Focus)
    }

    fn toggle_play(&mut self) {
        if !self.playing && self.position as usize >= self.last_frame && !self.looping {
            self.position = 0.;
        }
        self.playing = !self.playing;
    }

    /// A drag across the timeline scrubs the frame under the pointer, paused; a drag of
    /// the dock's slider seeks.
    fn seek_to_pointer(&mut self, studio: &StudioLayout, dock: &ControlsLayout, scale: f32) {
        let Some(pointer) = self.pointer else { return };
        if self.drag == Some(Control::Timeline) {
            self.position = studio.frame_at(studio.time_axis(pointer), self.last_frame + 1) as f64;
            self.playing = false;
        } else {
            self.position = (dock.progress_at(pointer.x * scale) * self.last_frame as f32)
                .round()
                .into();
        }
    }
}

#[test]
#[ignore = "writes a review film, run by hand with the FFRAMES_PLAYER_* variables"]
fn review_film() {
    let footage = std::env::var("FFRAMES_PLAYER_FOOTAGE").expect("FFRAMES_PLAYER_FOOTAGE");
    let out = std::env::var("FFRAMES_PLAYER_OUT").expect("FFRAMES_PLAYER_OUT");
    std::fs::create_dir_all(&out).expect("output directory");
    let size = std::env::var("FFRAMES_PLAYER_SIZE").unwrap_or_else(|_| "1280x720".to_owned());
    let (logical_width, logical_height) = size
        .split_once('x')
        .map(|(w, h)| (w.parse::<f32>().unwrap(), h.parse::<f32>().unwrap()))
        .expect("WxH");
    let scale: f32 = std::env::var("FFRAMES_PLAYER_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.);
    let with_motion = std::env::var("FFRAMES_PLAYER_MOTION").as_deref() != Ok("0");
    let (width, height) = (
        (logical_width * scale) as i32,
        (logical_height * scale) as i32,
    );

    let footage_frames = std::fs::read_dir(&footage)
        .expect("footage directory")
        .filter(|entry| {
            entry
                .as_ref()
                .is_ok_and(|e| e.path().extension().is_some_and(|ext| ext == "png"))
        })
        .count();
    assert!(footage_frames > 0, "no PNG frames in {footage}");
    let mut decoded: HashMap<usize, Image> = HashMap::new();
    let mut footage_frame = |frame: usize| -> Image {
        decoded
            .entry(frame % footage_frames)
            .or_insert_with(|| {
                let path = format!("{footage}/f_{:05}.png", frame % footage_frames);
                let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("read {path}"));
                Image::from_encoded(Data::new_copy(&bytes)).expect("decodable png")
            })
            .clone()
    };
    let video_size = {
        let first = footage_frame(0);
        usvgr::Size::from_wh(first.width() as f32, first.height() as f32).expect("size")
    };
    let aspect = video_size.width() / video_size.height();

    let duration_in_frames = footage_frames;
    let last_frame = duration_in_frames - 1;
    let third = duration_in_frames / 3;
    let lanes = vec![
        vec![("music.mp3", 0..duration_in_frames, envelope(7, 0.9))],
        vec![
            (
                "voice.wav",
                third / 2..duration_in_frames - third,
                envelope(3, 0.7),
            ),
            (
                "whoosh.wav",
                duration_in_frames - third / 2..duration_in_frames - third / 3,
                envelope(11, 1.),
            ),
        ],
    ];
    let mut model = Studio::fixture(
        FPS,
        duration_in_frames,
        &[
            ("Install", 0..third),
            ("Terminal", third..2 * third),
            ("Logo", 2 * third..duration_in_frames),
        ],
        &lanes,
        THUMBNAILS,
        aspect,
    );

    let controls = Controls::new();
    let dock = ControlsLayout::new(
        &controls,
        &format_time(duration_in_frames, FPS),
        width as f32,
        height as f32,
        scale,
    );
    let mut studio =
        StudioLayout::new(logical_width, logical_height, lanes.len(), aspect).expect("layout");

    let mut player = Player {
        playing: true,
        looping: false,
        studio_open: true,
        position: 0.,
        last_frame,
        pointer: None,
        hovered: None,
        pressed: None,
        drag: None,
        last_activity: -10.,
        controls_visible: false,
        travel: None,
    };
    let intent_of = |player: &Player| Intent {
        studio: player.studio_open,
        dock: player.controls_visible && !player.studio_open,
        playing: player.playing,
        looping: player.looping,
        hovered: if player.drag == Some(Control::Seek) {
            Some(Control::Seek)
        } else {
            player.hovered
        },
        pressed: player.pressed,
    };

    let base = Instant::now();
    let timing = if with_motion {
        Timing::DEFAULT
    } else {
        Timing::INSTANT
    };
    let mut motion = Motion::new(intent_of(&player), timing, base);
    let mut script = script().into_iter().peekable();
    let mut surface = surfaces::raster_n32_premul((width, height)).expect("surface");
    let scroll = StudioScroll::default();
    let mut thumbnails_arrived = 0;

    let total = (LENGTH * FPS as f32) as usize;
    let dt = 1. / FPS as f32;
    for index in 0..total {
        let t = index as f32 * dt;
        let now = base + Duration::from_secs_f32(t);

        // The thumbnails resolve left to right over the first second, as the workers
        // deliver them
        while thumbnails_arrived < THUMBNAILS && t >= 0.25 + thumbnails_arrived as f32 * 0.06 {
            let slot = thumbnails_arrived;
            let frame = (slot * 2 + 1) * duration_in_frames / (2 * THUMBNAILS);
            let image = footage_frame(frame);
            let thumb = surfaces::raster_n32_premul((192, (192. / aspect) as i32))
                .map(|mut s| {
                    s.canvas().draw_image_rect_with_sampling_options(
                        &image,
                        None,
                        Rect::from_wh(192., 192. / aspect),
                        SamplingOptions::from(FilterMode::Linear),
                        &Paint::default(),
                    );
                    s.image_snapshot()
                })
                .expect("thumbnail");
            model.set_thumbnail(slot, thumb, now);
            thumbnails_arrived += 1;
        }

        // The pointer travels along the travel curve
        if let Some((from, to, start, end)) = player.travel {
            let progress = Easing::TRAVEL.apply((t - start) / (end - start));
            let at = Point::new(
                from.x + (to.x - from.x) * progress,
                from.y + (to.y - from.y) * progress,
            );
            if player.pointer != Some(at) {
                player.pointer = Some(at);
                player.last_activity = t;
                if matches!(player.drag, Some(Control::Seek | Control::Timeline)) {
                    player.seek_to_pointer(&studio, &dock, scale);
                }
            }
            if t >= end {
                player.travel = None;
            }
        }

        while script.peek().is_some_and(|(at, _)| *at <= t) {
            let (_, event) = script.next().unwrap();
            match event {
                Event::Enter(point) => {
                    player.pointer = Some(point);
                    player.last_activity = t;
                }
                Event::MoveTo(target, end) => {
                    // Off the glyph, like a hand that does not cover what it presses
                    let off_glyph = |rect: Rect| {
                        Point::new(
                            rect.center_x() + rect.width() * 0.18,
                            rect.center_y() + rect.height() * 0.2,
                        )
                    };
                    let to = match target {
                        Target::Button(control) if player.studio_open => {
                            off_glyph(studio.button_rect(control).expect("transport button"))
                        }
                        Target::Button(control) => {
                            let rect = match control {
                                Control::PlayPause => dock.play,
                                Control::Loop => dock.looping,
                                Control::Back => dock.back,
                                Control::Forward => dock.forward,
                                Control::Fullscreen => dock.fullscreen,
                                _ => dock.track,
                            };
                            let p = off_glyph(rect);
                            Point::new(p.x / scale, p.y / scale)
                        }
                        Target::Scene(frame) => studio.scene_point(frame, duration_in_frames),
                        Target::Strip(frame) => studio.strip_point(frame, duration_in_frames),
                        Target::Restore => off_glyph(studio.restore_rect()),
                        Target::Point(point) => point,
                    };
                    let from = player.pointer.unwrap_or(to);
                    player.travel = Some((from, to, t, end));
                }
                Event::Press => {
                    let control = player.control_under_cursor(&studio, &model, &dock, scale);
                    player.pressed = control.filter(|c| {
                        matches!(
                            c,
                            Control::Back
                                | Control::PlayPause
                                | Control::Forward
                                | Control::Loop
                                | Control::Fullscreen
                                | Control::Focus
                        )
                    });
                    match control {
                        Some(Control::PlayPause) => player.toggle_play(),
                        Some(control @ (Control::Seek | Control::Timeline)) => {
                            player.drag = Some(control);
                            player.seek_to_pointer(&studio, &dock, scale);
                        }
                        Some(control @ Control::Scene(_)) => player.drag = Some(control),
                        Some(Control::Focus) => {
                            player.studio_open = !player.studio_open;
                            player.hovered = None;
                            player.last_activity = t;
                        }
                        Some(Control::Loop) => player.looping = !player.looping,
                        Some(_) => {}
                        None => {
                            player.toggle_play();
                            motion.flash(player.playing, now);
                        }
                    }
                }
                Event::Release => {
                    player.pressed = None;
                    let drag = player.drag.take();
                    if let Some(control @ Control::Scene(start)) = drag
                        && player.control_under_cursor(&studio, &model, &dock, scale)
                            == Some(control)
                    {
                        player.position = start as f64;
                        player.playing = false;
                    }
                }
                Event::Space => {
                    player.toggle_play();
                    motion.flash(player.playing, now);
                }
            }
        }

        player.controls_visible = player.controls_should_be_visible(t);
        player.hovered = player.control_under_cursor(&studio, &model, &dock, scale);
        player.controls_visible = player.controls_should_be_visible(t);
        motion.follow(intent_of(&player), now);

        let frame = player.position as usize;
        let state = ControlsState {
            frame,
            duration_in_frames,
            fps: FPS,
            fullscreen: false,
            motion: motion.sample(now),
        };
        let image = footage_frame(frame);
        draw_frame(
            surface.canvas(),
            &controls,
            &dock,
            &mut studio,
            &model,
            &scroll,
            &state,
            &image,
            video_size,
            scale,
        );

        let cursor_hidden = with_motion && !player.studio_open && !player.controls_visible;
        if let (Some(pointer), false) = (player.pointer, cursor_hidden) {
            draw_cursor(
                surface.canvas(),
                Point::new(pointer.x * scale, pointer.y * scale),
                scale,
            );
        }

        let png = surface
            .image_snapshot()
            .encode(None, EncodedImageFormat::PNG, 100)
            .expect("png");
        std::fs::write(format!("{out}/{index:05}.png"), png.as_bytes()).expect("write png");

        if player.playing {
            player.position += 1.;
            if player.position as usize > player.last_frame {
                if player.looping {
                    player.position = 0.;
                } else {
                    player.position = player.last_frame as f64;
                    player.playing = false;
                }
            }
        }
    }
}

/// One frame of the window, the way `Presenter::present` composes it.
#[allow(clippy::too_many_arguments)]
fn draw_frame(
    canvas: &Canvas,
    controls: &Controls,
    dock: &ControlsLayout,
    studio: &mut StudioLayout,
    model: &Studio,
    scroll: &StudioScroll,
    state: &ControlsState,
    image: &Image,
    video_size: usvgr::Size,
    scale: f32,
) {
    let view = state.motion.studio;
    canvas.clear(mix(Color::BLACK, Color::from_rgb(14, 14, 14), view));
    let size = canvas.base_layer_size();
    let window = Rect::from_iwh(size.width, size.height);
    let picture = crate::presenter::video_rect(Some(video_size), window, Some(studio), view, scale)
        .expect("video rect");
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    canvas.draw_image_rect_with_sampling_options(
        image,
        None,
        picture,
        SamplingOptions::from(FilterMode::Linear),
        &paint,
    );
    if let Some((progress, playing)) = state.motion.flash {
        controls.flash(canvas, picture.center(), scale, progress, playing);
    }

    canvas.save();
    canvas.scale((scale, scale));
    if view > 0. {
        let (dx, dy) = studio.closed_offset();
        canvas.save();
        canvas.translate((dx * (1. - view), dy * (1. - view)));
        studio.draw(canvas, controls, state, model, scroll);
        canvas.restore();
    }
    studio.draw_restore(
        canvas,
        controls,
        state.motion.hover(Control::Focus),
        state.motion.dock * (1. - view),
    );
    canvas.restore();
    controls.draw(canvas, dock, state);
}

/// A clip envelope in unit coordinates, like `studio::waveform` makes from samples.
fn envelope(seed: u32, gain: f32) -> fframes_skia_renderer::skia_safe::Path {
    const BINS: usize = 512;
    let mut path = PathBuilder::new();
    path.move_to((0., 1.));
    let mut x = seed.wrapping_mul(2_654_435_761);
    for i in 0..BINS {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let noise = (x % 1000) as f32 / 1000.;
        let beat = ((i as f32 / 8.).sin() * 0.5 + 0.5).powi(3);
        let peak = ((0.15 + 0.55 * beat + 0.3 * noise) * gain).min(1.);
        path.line_to(((i as f32 + 0.5) / BINS as f32, 1. - peak));
    }
    path.line_to((1., 1.)).close();
    path.detach()
}

/// The macOS arrow, so the film shows what the chrome reacts to.
fn draw_cursor(canvas: &Canvas, at: Point, scale: f32) {
    const ARROW: [[f32; 2]; 7] = [
        [0., 0.],
        [0., 16.5],
        [4.2, 12.6],
        [7.2, 19.],
        [9.9, 17.8],
        [6.9, 11.5],
        [12.4, 11.5],
    ];
    let mut path = PathBuilder::new();
    path.move_to(Point::new(ARROW[0][0], ARROW[0][1]));
    for [x, y] in &ARROW[1..] {
        path.line_to(Point::new(*x, *y));
    }
    path.close();
    let path = path.detach();

    canvas.save();
    canvas.translate((at.x, at.y));
    canvas.scale((scale, scale));
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::WHITE);
    paint.set_style(PaintStyle::StrokeAndFill);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_stroke_width(1.6);
    canvas.draw_path(&path, &paint);
    paint.set_style(PaintStyle::Fill);
    paint.set_color(Color::BLACK);
    canvas.draw_path(&path, &paint);
    canvas.restore();
}
