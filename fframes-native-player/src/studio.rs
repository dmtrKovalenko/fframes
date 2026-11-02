//! Scene navigation and the clip timeline for the native preview. All layout and pointer
//! coordinates here are logical pixels, regardless of the window's backing scale.

use std::ops::Range;
use std::time::{Duration, Instant};

use fframes::{
    AudioData, AudioTimelineSamples, AudioTimelineUnit, MediaProvider, ResolvedRenderingTimeline,
    TimeBase, TimelineIndex, media::PreloadedAudioData,
};
use fframes_skia_renderer::skia_safe::{
    Canvas, Color, Contains, FilterMode, Font, IRect, Image, ImageInfo, Paint, Path, PathBuilder,
    Point, RRect, Rect, RoundOut, SamplingOptions, surfaces,
};

use crate::controls::{
    Control, Controls, ControlsState, draw_tabular, fade, format_time, tabular_width,
};

pub(crate) struct Studio {
    pub index: TimelineIndex,
    pub video_aspect: f32,
    audio: Vec<Vec<Clip>>,
    thumbnails: Vec<Option<Image>>,
    /// When each thumbnail arrived, so it can fade in instead of popping.
    thumbnail_arrivals: Vec<Option<Instant>>,
    thumbnail_revision: u64,
}

struct Clip {
    name: String,
    frames: Range<usize>,
    waveform: Option<Path>,
}

impl Studio {
    pub fn new<'m>(
        time: TimeBase,
        timeline: &ResolvedRenderingTimeline<'_, AudioTimelineSamples>,
        media: Option<&'m dyn MediaProvider<'m>>,
        thumbnail_count: usize,
        video_aspect: f32,
    ) -> Self {
        let index = TimelineIndex::new(
            time.fps,
            timeline.duration_in_frames,
            timeline.scenes.as_ref(),
        );
        let mut clips = Vec::new();
        if let Some(audio) = &timeline.audio_map {
            for track in audio.tracks() {
                clips.push(Clip {
                    name: track.file.clone(),
                    waveform: match media.and_then(|m| m.resolve_audio(&track.file)) {
                        Some(AudioData::Preloaded(data)) => {
                            let seconds = track.range.end.to_seconds(&time)
                                - track.range.start.to_seconds(&time);
                            Some(waveform(
                                data,
                                f64::from(track.mix.offset),
                                seconds * f64::from(track.mix.rate),
                            ))
                        }
                        _ => None,
                    },
                    frames: track.range.start.to_frames(&time)..track.range.end.to_frames(&time),
                });
            }
        }
        Self {
            index,
            video_aspect,
            audio: pack_clips(clips),
            thumbnails: vec![None; thumbnail_count],
            thumbnail_arrivals: vec![None; thumbnail_count],
            thumbnail_revision: 0,
        }
    }

    pub fn set_thumbnail(&mut self, slot: usize, image: Image, now: Instant) {
        self.thumbnails[slot] = Some(image);
        self.thumbnail_arrivals[slot] = Some(now);
        self.thumbnail_revision += 1;
    }

    /// Whether a thumbnail that arrived less than `fade` ago is still fading in.
    pub fn is_fading_in(&self, now: Instant, fade: Duration) -> bool {
        !fade.is_zero()
            && self
                .thumbnail_arrivals
                .iter()
                .flatten()
                .any(|arrival| now.saturating_duration_since(*arrival) < fade)
    }

    /// How far each thumbnail has faded in at `now`, `1.0` once it is fully there.
    fn thumbnail_presence(&self, state: &ControlsState) -> impl Iterator<Item = (usize, f32)> {
        let motion = &state.motion;
        self.thumbnail_arrivals
            .iter()
            .enumerate()
            .filter_map(move |(slot, arrival)| {
                let arrival = (*arrival)?;
                let elapsed = motion.now.saturating_duration_since(arrival);
                if motion.thumbnail_fade.is_zero() || elapsed >= motion.thumbnail_fade {
                    return None;
                }
                Some((
                    slot,
                    elapsed.as_secs_f32() / motion.thumbnail_fade.as_secs_f32(),
                ))
            })
    }

    pub fn adjacent_scene(&self, frame: usize, next: bool) -> Option<usize> {
        let starts = self
            .index
            .scenes
            .iter()
            .map(|scene| scene.frames.start)
            .filter(|&start| start < self.index.duration_in_frames);
        if next {
            starts.filter(|&start| start > frame).min()
        } else {
            starts.filter(|&start| start < frame).max()
        }
    }

    pub fn audio_rows(&self) -> usize {
        self.audio.len()
    }
}

/// Rasterize only the small overview image on the frame worker that owns the decoder.
pub(crate) fn thumbnail(tree: &fframes::usvgr::Tree, bg: fframes::Color) -> Option<Image> {
    let video = tree.size();
    let scale = (192. / video.width()).min(108. / video.height());
    let size = (
        (video.width() * scale).ceil() as i32,
        (video.height() * scale).ceil() as i32,
    );
    let mut surface = surfaces::raster_n32_premul(size)?;
    let canvas = surface.canvas();
    canvas.clear(Color::from_argb(bg.a, bg.r, bg.g, bg.b));
    canvas.scale((scale, scale));
    fframes_skia_renderer::render::render_tree(
        tree,
        canvas,
        &mut fframes_skia_renderer::render::RenderCache::new(),
    );
    Some(surface.image_snapshot())
}

// A source envelope in unit coordinates, computed once per clip. Include both channels
// and keep silence past EOF when a cue extends beyond the source.
fn waveform(data: &PreloadedAudioData<'_>, offset: f64, seconds: f64) -> Path {
    const BINS: usize = 512;
    let rate = f64::from(data.sample_rate);
    let (left, right) = data.channels();
    let mut path = PathBuilder::new();
    path.move_to((0., 1.));
    let sample = |i: usize| ((offset + seconds * i as f64 / BINS as f64) * rate) as usize;
    for i in 0..BINS {
        let start = sample(i).min(left.len());
        let end = sample(i + 1).min(left.len());
        // Bound startup work even for a multi-hour file.
        let step = end.saturating_sub(start).div_ceil(64).max(1);
        let peak = (start..end)
            .step_by(step)
            .map(|j| left[j].abs().max(right.get(j).copied().unwrap_or(0.).abs()))
            .fold(0., f32::max)
            .min(1.);
        path.line_to(((i as f32 + 0.5) / BINS as f32, 1. - peak));
    }
    path.line_to((1., 1.)).close();
    path.detach()
}

// Put simultaneous sounds on separate rows while letting disjoint clips share a row.
fn pack_clips(mut clips: Vec<Clip>) -> Vec<Vec<Clip>> {
    clips.sort_by_key(|clip| clip.frames.start);
    let mut lanes: Vec<Vec<Clip>> = Vec::new();
    for clip in clips {
        if let Some(lane) = lanes.iter_mut().find(|lane| {
            lane.last()
                .is_some_and(|last| last.frames.end <= clip.frames.start)
        }) {
            lane.push(clip);
        } else {
            lanes.push(vec![clip]);
        }
    }
    lanes
}

const SURFACE: Color = Color::from_rgb(23, 23, 23);
const CANVAS: Color = Color::from_rgb(14, 14, 14);
const LINE: Color = Color::from_rgb(58, 58, 58);
const TEXT: Color = Color::from_rgb(249, 249, 249);
const QUIET: Color = Color::from_rgb(174, 174, 174);
const ORANGE: Color = Color::from_rgb(251, 106, 34);
// Theme.scenes in the legacy web studio. Orange remains reserved for the playhead.
const SCENE_COLORS: [Color; 6] = [
    Color::from_rgb(2, 133, 255),
    Color::from_rgb(4, 184, 76),
    Color::from_rgb(146, 79, 247),
    Color::from_rgb(255, 102, 173),
    Color::from_rgb(255, 195, 0),
    Color::from_rgb(250, 66, 62),
];
const AUDIO: Color = Color::from_rgb(45, 45, 45);
const TRANSPORT: f32 = 50.;
const RULER: f32 = 22.;
const SCENE_HEIGHT: f32 = 24.;
const LANE: f32 = 31.;
const MARGIN: f32 = 16.;
pub(crate) const PORTRAIT_SIDEBAR_WIDTH: f32 = 360.;

#[derive(Default)]
pub(crate) struct StudioScroll {
    pub audio: usize,
}

pub(crate) struct StudioLayout {
    pub stage: Rect,
    timeline: Rect,
    /// The pill of the focus view that brings the studio back.
    restore: Rect,
    transport: Rect,
    pub portrait: bool,
    audio_slots: usize,
    film_height: f32,
    timeline_cache: Option<TimelineCache>,
}

struct TimelineCache {
    image: Image,
    bounds: IRect,
    /// Where the canvas was: the panel moves while the views change, and the cache is
    /// made again at each position (a few frames, the studio draws in about a millisecond).
    translation: (f32, f32),
    frames: Range<usize>,
    thumbnail_revision: u64,
    audio_scroll: usize,
    scale: (f32, f32),
}

impl StudioLayout {
    pub fn new(width: f32, height: f32, audio_rows: usize, video_aspect: f32) -> Option<Self> {
        if width < 320. || height < 240. {
            return None;
        }
        let restore = Rect::from_xywh(width - 112., 10., 96., 30.);
        if video_aspect < 1. {
            let panel = (width - height * video_aspect)
                .clamp((width * 0.38).min(320.), (width * 0.60).min(416.));
            let narrow = panel < 320.;
            let bottom = if narrow { height - TRANSPORT } else { height };
            let transport = if narrow {
                Rect::from_xywh(0., bottom, width, TRANSPORT)
            } else {
                Rect::from_xywh(0., 0., panel, TRANSPORT)
            };
            return Some(Self {
                stage: Rect::from_ltrb(panel, 0., width, bottom),
                timeline: Rect::from_ltrb(0., 0., panel, bottom),
                transport,
                restore,
                portrait: true,
                audio_slots: audio_rows.min(if panel >= 260. { 3 } else { 1 }),
                film_height: 0.,
                timeline_cache: None,
            });
        }
        let audio_slots = audio_rows.min(if height >= 600. {
            3
        } else {
            usize::from(height >= 340.)
        });
        let film_height = if height >= 480. {
            58.
        } else if height >= 340. {
            36.
        } else {
            0.
        };
        let timeline_y = height
            - TRANSPORT
            - RULER
            - SCENE_HEIGHT
            - film_height
            - 16.
            - LANE * audio_slots as f32;
        Some(Self {
            stage: Rect::from_ltrb(0., 0., width, timeline_y),
            timeline: Rect::from_ltrb(0., timeline_y, width, height),
            restore,
            transport: Rect::from_xywh(0., timeline_y, width, TRANSPORT),
            portrait: false,
            audio_slots,
            film_height,
            timeline_cache: None,
        })
    }

    fn tracks(&self) -> Rect {
        if self.portrait {
            let top = if self.transport.top == 0. {
                self.transport.bottom
            } else {
                0.
            };
            return Rect::from_ltrb(
                6.,
                top + 18.,
                self.timeline.right - 6.,
                self.timeline.bottom - 10.,
            );
        }
        Rect::from_ltrb(
            48.,
            self.timeline.top + TRANSPORT + 2.,
            self.timeline.right - MARGIN,
            self.timeline.bottom - 5.,
        )
    }

    // Time runs down the panel. The same columns define the pixels and the hit targets.
    fn portrait_columns(&self) -> (Rect, Rect, Rect) {
        let track = self.tracks();
        let film_x = (track.width() * 0.14).clamp(30., 46.) + track.left;
        let film = Rect::from_xywh(
            film_x,
            track.top,
            (track.width() * 0.16).clamp(22., 52.),
            track.height(),
        );
        let audio_x = track.right - 18. * self.audio_slots as f32;
        let scenes = Rect::from_ltrb(film.right + 6., track.top, audio_x - 3., track.bottom);
        let audio = Rect::from_ltrb(audio_x, track.top, track.right, track.bottom);
        (film, scenes, audio)
    }

    fn draw_portrait_tracks(
        &self,
        ui: &Ink<'_>,
        state: &ControlsState,
        model: &Studio,
        scroll: &StudioScroll,
    ) {
        let track = self.tracks();
        let (film, scenes, mut audio) = self.portrait_columns();
        if model.index.scenes.is_empty() {
            audio.left = scenes.left;
        }
        let lane_width = audio.width() / self.audio_slots.max(1) as f32;
        let duration = state.duration_in_frames;
        let at = |f| self.frame_position(f, duration);
        let canvas = ui.canvas;
        ui.fill(
            Rect::from_xywh(self.timeline.right - 1., 0., 1., self.timeline.bottom),
            LINE,
        );
        for frame in (0..=duration).step_by(tick_step(duration, state.fps, track.height(), 64.)) {
            let y = at(frame);
            let label = if duration < state.fps {
                format!("{frame}f")
            } else {
                format_time(frame, state.fps)
            };
            ui.clipped_text(
                &label,
                Rect::from_ltrb(track.left, y - 8., film.left - 3., y + 10.),
                y + 4.,
                11.,
                QUIET,
                false,
            );
            ui.fill(Rect::from_xywh(film.left - 4., y, 4., 1.), LINE);
        }
        if !model.thumbnails.is_empty() {
            let h = film.height() / model.thumbnails.len() as f32;
            canvas.save();
            canvas.clip_rrect(RRect::new_rect_xy(film, 4., 4.), None, true);
            ui.fill(film, CANVAS);
            for (i, image) in model.thumbnails.iter().enumerate() {
                if let Some(image) = image {
                    ui.thumbnail(
                        image,
                        Rect::from_xywh(film.left, film.top + i as f32 * h, film.width(), h),
                    );
                }
            }
            canvas.restore();
        }
        for scene in &model.index.scenes {
            let top = at(scene.frames.start);
            let bottom = at(scene.frames.end);
            if bottom <= top {
                continue;
            }
            let active = scene.frames.contains(&state.frame);
            let color = SCENE_COLORS[scene.index % SCENE_COLORS.len()];
            let band = Rect::from_ltrb(scenes.left, top, scenes.right, (bottom - 1.).max(top));
            ui.fill(band, color.with_a(if active { 34 } else { 15 }));
            ui.fill(
                Rect::from_ltrb(band.left, top, band.left + 3., band.bottom),
                color,
            );
            if band.height() >= 13. {
                let label = Rect::from_ltrb(band.left + 8., top, band.right - 2., band.bottom);
                ui.clipped_text(
                    &scene.name,
                    label,
                    top + 12.,
                    11.,
                    if active { TEXT } else { QUIET },
                    active,
                );
            }
        }
        let first = scroll
            .audio
            .min(model.audio_rows().saturating_sub(self.audio_slots));
        for (i, lane) in model
            .audio
            .iter()
            .skip(first)
            .take(self.audio_slots)
            .enumerate()
        {
            let x = audio.left + lane_width * i as f32;
            ui.text(
                &format!("A{}", first + i + 1),
                x,
                track.top - 4.,
                10.,
                QUIET,
                false,
            );
            for clip in lane {
                let r = Rect::from_ltrb(
                    x,
                    at(clip.frames.start),
                    x + lane_width - 3.,
                    at(clip.frames.end),
                );
                ui.round(r, AUDIO, 3.);
                if let Some(path) = &clip.waveform {
                    canvas.save();
                    canvas.translate((r.right - 1., r.top));
                    canvas.rotate(90., None);
                    canvas.scale((r.height(), r.width() - 2.));
                    canvas.draw_path(path, &Ink::paint(Color::from_rgb(86, 86, 86)));
                    canvas.restore();
                }
                if r.width() >= 55. && r.height() >= 17. {
                    let label = Rect::from_ltrb(r.left + 5., r.top, r.right - 3., r.bottom);
                    ui.clipped_text(&clip.name, label, r.top + 13., 11., TEXT, false);
                }
            }
        }
    }

    fn buttons(&self) -> [(Control, Rect); 6] {
        let width = self.transport.width();
        let narrow = width < 460.;
        let size = if narrow { 26. } else { 32. };
        let play_size = if narrow { 30. } else { 36. };
        let gap = if narrow { 2. } else { 5. };
        let margin = if narrow { 8. } else { MARGIN };
        let mid = width / 2.;
        let y = self.transport.center_y();
        let button = |x, w, h| Rect::from_xywh(x, y - h / 2., w, h);
        let play = button(mid - play_size / 2., play_size, play_size);
        let focus = button(width - margin - size, size, size);
        let full = button(focus.left - gap - size, size, size);
        [
            (Control::Back, button(play.left - gap - size, size, size)),
            (Control::PlayPause, play),
            (Control::Forward, button(play.right + gap, size, size)),
            (Control::Loop, button(full.left - gap - size, size, size)),
            (Control::Fullscreen, full),
            (Control::Focus, focus),
        ]
    }

    /// The translation that moves the whole panel off its edge of the window, so the
    /// studio can slide away as the focus view takes over.
    pub fn closed_offset(&self) -> (f32, f32) {
        if self.portrait {
            (-self.timeline.right, 0.)
        } else {
            (0., self.timeline.bottom - self.timeline.top)
        }
    }

    pub fn time_axis(&self, point: Point) -> f32 {
        if self.portrait { point.y } else { point.x }
    }

    fn time_bounds(&self) -> (f32, f32) {
        let track = self.tracks();
        if self.portrait {
            (track.top, track.height())
        } else {
            (track.left, track.width())
        }
    }

    fn frame_position(&self, frame: usize, duration: usize) -> f32 {
        let (start, length) = self.time_bounds();
        start + length * (frame.min(duration) as f32 / duration.max(1) as f32)
    }

    pub fn frame_at(&self, position: f32, duration: usize) -> usize {
        let (start, length) = self.time_bounds();
        let fraction = ((position - start) / length).clamp(0., 1.);
        ((fraction * duration as f32).floor() as usize).min(duration.saturating_sub(1))
    }

    pub fn hit(&self, point: Point, model: &Studio) -> Option<Control> {
        if let Some((control, _)) = self.buttons().into_iter().find(|(_, r)| r.contains(point)) {
            return Some(control);
        }
        let tracks = self.tracks();
        if tracks.contains(point) {
            let in_scenes = if self.portrait {
                self.portrait_columns().1.contains(point)
            } else {
                (tracks.top + RULER..tracks.top + RULER + SCENE_HEIGHT).contains(&point.y)
            };
            if in_scenes {
                let frame = self.frame_at(self.time_axis(point), model.index.duration_in_frames);
                if let Some(scene) = model.index.scenes_at(frame).last() {
                    return Some(Control::Scene(scene.frames.start));
                }
            }
            return Some(Control::Timeline);
        }
        (!self.stage.contains(point)).then_some(Control::Bar)
    }

    pub fn scroll(&self, model: &Studio, scroll: &mut StudioScroll, point: Point, steps: isize) {
        if self.tracks().contains(point) {
            scroll.audio = scroll
                .audio
                .saturating_add_signed(steps)
                .min(model.audio_rows().saturating_sub(self.audio_slots));
        }
    }

    pub fn focus_hit(&self, point: Point) -> bool {
        self.restore.contains(point)
    }

    /// The pill that brings the studio back, shown with the focus dock: `reveal` fades it
    /// with the dock, `hover` lights it.
    pub fn draw_restore(&self, canvas: &Canvas, controls: &Controls, hover: f32, reveal: f32) {
        if reveal <= 0. {
            return;
        }
        let ui = Ink::new(canvas, controls);
        ui.round(
            self.restore,
            fade(crate::controls::mix(SURFACE, LINE, hover), reveal),
            7.,
        );
        ui.text(
            "Studio  ·  T",
            self.restore.left + 13.,
            30.,
            13.,
            fade(TEXT, reveal),
            true,
        );
    }

    pub fn draw(
        &mut self,
        canvas: &Canvas,
        controls: &Controls,
        state: &ControlsState,
        model: &Studio,
        scroll: &StudioScroll,
    ) {
        let ui = Ink::new(canvas, controls);
        self.draw_cached_timeline(&ui, state, model, scroll);
        self.draw_arriving_thumbnails(&ui, state, model);
        self.draw_scene_hover(&ui, state, model);
        self.draw_transport(&ui, controls, state);
        self.draw_playhead(&ui, state);
    }

    /// The rectangle of every thumbnail slot and the rounded strip that clips them.
    fn thumbnail_slots(&self, count: usize) -> Option<(RRect, Vec<Rect>)> {
        if count == 0 {
            return None;
        }
        let track = self.tracks();
        if self.portrait {
            let film = self.portrait_columns().0;
            let h = film.height() / count as f32;
            let slots = (0..count)
                .map(|i| Rect::from_xywh(film.left, film.top + i as f32 * h, film.width(), h))
                .collect();
            return Some((RRect::new_rect_xy(film, 4., 4.), slots));
        }
        if self.film_height <= 0. {
            return None;
        }
        let strip_top = track.top + RULER + SCENE_HEIGHT;
        let strip = Rect::from_ltrb(
            track.left,
            strip_top,
            track.right,
            strip_top + self.film_height,
        );
        let w = strip.width() / count as f32;
        let slots = (0..count)
            .map(|i| Rect::from_xywh(track.left + i as f32 * w, strip.top, w, self.film_height))
            .collect();
        Some((RRect::new_rect_xy(strip, 5., 5.), slots))
    }

    /// A thumbnail that just arrived is still covered by the strip's own color, which
    /// clears over the fade: the picture resolves instead of popping in.
    fn draw_arriving_thumbnails(&self, ui: &Ink<'_>, state: &ControlsState, model: &Studio) {
        let arriving: Vec<(usize, f32)> = model.thumbnail_presence(state).collect();
        if arriving.is_empty() {
            return;
        }
        let Some((clip, slots)) = self.thumbnail_slots(model.thumbnails.len()) else {
            return;
        };
        let canvas = ui.canvas;
        canvas.save();
        canvas.clip_rrect(clip, None, true);
        for (slot, presence) in arriving {
            ui.fill(slots[slot], fade(CANVAS, 1. - presence));
        }
        canvas.restore();
    }

    /// The scene under the pointer is lit a step above its band, around the film strip.
    fn draw_scene_hover(&self, ui: &Ink<'_>, state: &ControlsState, model: &Studio) {
        let Some((start, lit)) = state.motion.scene else {
            return;
        };
        let Some(scene) = model
            .index
            .scenes
            .iter()
            .find(|scene| scene.frames.start == start)
        else {
            return;
        };
        let duration = state.duration_in_frames;
        let color = fade(
            SCENE_COLORS[scene.index % SCENE_COLORS.len()].with_a(26),
            lit,
        );
        let track = self.tracks();
        if self.portrait {
            let scenes = self.portrait_columns().1;
            let top = self.frame_position(scene.frames.start, duration);
            let bottom = self.frame_position(scene.frames.end, duration);
            ui.fill(
                Rect::from_ltrb(scenes.left, top, scenes.right, (bottom - 1.).max(top)),
                color,
            );
            return;
        }
        let left = self.frame_position(scene.frames.start, duration);
        let right = self.frame_position(scene.frames.end, duration);
        let scene_top = track.top + RULER;
        let strip_top = scene_top + SCENE_HEIGHT;
        ui.fill(Rect::from_ltrb(left, scene_top, right, strip_top), color);
        ui.fill(
            Rect::from_ltrb(left, strip_top + self.film_height, right, track.bottom),
            color,
        );
    }

    fn draw_transport(&self, ui: &Ink<'_>, controls: &Controls, state: &ControlsState) {
        let canvas = ui.canvas;
        ui.fill(self.transport, SURFACE);
        ui.fill(
            Rect::from_xywh(0., self.transport.top, self.transport.width(), 1.),
            LINE,
        );
        let buttons = self.buttons();
        let center = self.transport.center_y();
        let compact = self.transport.width() < 460.;
        let left = if compact { 8. } else { MARGIN };
        let total = format_time(state.duration_in_frames, state.fps);
        let time = format_time(state.frame, state.fps);
        let size: f32 = if compact { 14. } else { 17. };
        let frame_size = (size * 0.74).min(if compact { 11. } else { 12. });
        let mut font = ui.medium.with_size(size).unwrap();
        let mut frame_font = ui.font.with_size(frame_size).unwrap();
        let mark_size = controls.frame_mark.root().intrinsic_size();
        let mark_aspect = mark_size.width / mark_size.height;
        let mark_height_ratio = 1.5;
        let inline_gap = if compact { 6. } else { 10. };
        let mut room = buttons[0].1.left - left - 6.;
        let mut readout_width = tabular_width(&font, &format!("{total} / {total}"));
        if !self.portrait {
            room -= inline_gap + 2.;
            let mark_width = frame_font.metrics().1.cap_height * mark_height_ratio * mark_aspect;
            let last_frame = state.duration_in_frames.saturating_sub(1);
            readout_width += mark_width + tabular_width(&frame_font, &last_frame.to_string());
        }
        if readout_width > room {
            let scale = room / readout_width;
            font = font.with_size(size * scale).unwrap();
            frame_font = frame_font.with_size(frame_size * scale).unwrap();
        }
        let frame_height = frame_font.metrics().1.cap_height;
        let clock_height = font.metrics().1.cap_height;
        let second_row = if self.portrait { 4. + frame_height } else { 0. };
        let baseline = center + (clock_height - second_row) / 2.;
        let after_time = draw_tabular(
            canvas,
            &time,
            Point::new(left, baseline),
            &font,
            &Ink::paint(TEXT),
        );
        let after_clock = draw_tabular(
            canvas,
            &format!(" / {total}"),
            Point::new(after_time, baseline),
            &font,
            &Ink::paint(QUIET),
        );
        let frame_left = if self.portrait {
            left
        } else {
            after_clock + inline_gap
        };
        let frame_baseline = baseline + second_row;
        let mark_height = frame_height * mark_height_ratio;
        let scale = mark_height / mark_size.height;
        canvas.save();
        canvas.translate((frame_left, frame_baseline - mark_height * 0.85));
        canvas.scale((scale, scale));
        controls.frame_mark.render(canvas);
        canvas.restore();
        draw_tabular(
            canvas,
            &state.frame.to_string(),
            Point::new(frame_left + mark_height * mark_aspect + 2., frame_baseline),
            &frame_font,
            &Ink::paint(QUIET),
        );

        for (control, rect) in buttons {
            controls.draw_button(canvas, control, rect, state, 1., 1.);
        }
        ui.fill(
            Rect::from_xywh(0., self.transport.bottom - 1., self.transport.width(), 1.),
            LINE,
        );
    }

    fn draw_cached_timeline(
        &mut self,
        ui: &Ink<'_>,
        state: &ControlsState,
        model: &Studio,
        scroll: &StudioScroll,
    ) {
        let canvas = ui.canvas;
        let matrix = canvas.local_to_device_as_3x3();
        let scale = (matrix.scale_x(), matrix.scale_y());
        let translation = (matrix.translate_x(), matrix.translate_y());
        let valid = self.timeline_cache.as_ref().is_some_and(|cache| {
            cache.frames.contains(&state.frame)
                && cache.translation == translation
                && cache.thumbnail_revision == model.thumbnail_revision
                && cache.audio_scroll == scroll.audio
                && cache.scale == scale
        });
        if !valid {
            // Align the panel to device pixels so its labels stay sharp at fractional DPI.
            let bounds: IRect = matrix.map_rect(self.timeline).0.round_out();
            let info = ImageInfo::new_n32_premul(bounds.size(), None);
            self.timeline_cache = canvas.new_surface(&info, None).map(|mut surface| {
                let target = surface.canvas();
                target.clear(Color::TRANSPARENT);
                target.translate((-bounds.left as f32, -bounds.top as f32));
                target.concat(&matrix);
                self.draw_tracks(
                    &Ink {
                        canvas: target,
                        ..ui.clone()
                    },
                    state,
                    model,
                    scroll,
                );

                // Scene labels change color at either end of an overlapping scene.
                let mut frames = 0..usize::MAX;
                for scene in &model.index.scenes {
                    for boundary in [scene.frames.start, scene.frames.end] {
                        if boundary <= state.frame {
                            frames.start = frames.start.max(boundary);
                        } else {
                            frames.end = frames.end.min(boundary);
                        }
                    }
                }
                TimelineCache {
                    image: surface.image_snapshot(),
                    bounds,
                    translation,
                    frames,
                    thumbnail_revision: model.thumbnail_revision,
                    audio_scroll: scroll.audio,
                    scale,
                }
            });
        }
        if let Some(cache) = &self.timeline_cache {
            canvas.save();
            canvas.reset_matrix();
            canvas.draw_image(
                &cache.image,
                (cache.bounds.left as f32, cache.bounds.top as f32),
                None,
            );
            canvas.restore();
        } else {
            self.draw_tracks(ui, state, model, scroll);
        }
    }

    fn draw_tracks(
        &self,
        ui: &Ink<'_>,
        state: &ControlsState,
        model: &Studio,
        scroll: &StudioScroll,
    ) {
        let canvas = ui.canvas;
        ui.fill(self.timeline, SURFACE);
        let track = self.tracks();
        if self.portrait {
            return self.draw_portrait_tracks(ui, state, model, scroll);
        }
        let scene_top = track.top + RULER;
        let duration = state.duration_in_frames;
        let fps = state.fps.max(1);
        let step = tick_step(duration, fps, track.width(), 100.);
        for frame in (0..=duration).step_by(step) {
            let x = self.frame_position(frame, duration);
            let time = if duration < fps {
                format!("{frame}f")
            } else {
                format_time(frame, state.fps)
            };
            ui.text(
                &time,
                x.min(track.right - 37.),
                track.top + 13.,
                11.,
                QUIET,
                false,
            );
            ui.fill(Rect::from_ltrb(x, track.top + 16., x + 1., scene_top), LINE);
        }

        let strip_top = scene_top + SCENE_HEIGHT;
        let mut previous_end = 0;
        for scene in &model.index.scenes {
            let left = self.frame_position(scene.frames.start, duration);
            let right = self.frame_position(scene.frames.end, duration);
            if right <= left {
                continue;
            }
            // Keep the earlier scene's end notch legible during a crossfade.
            let rail = self
                .frame_position(previous_end.max(scene.frames.start), duration)
                .min(right);
            previous_end = scene.frames.end;
            let color = SCENE_COLORS[scene.index % SCENE_COLORS.len()];
            ui.fill(
                Rect::from_ltrb(left, scene_top, right, track.bottom),
                color.with_a(13),
            );
            ui.fill(
                Rect::from_ltrb(rail, scene_top, right, scene_top + 3.),
                color,
            );
            let notch = (right - rail).min(9.);
            let mut marker = PathBuilder::new();
            marker.move_to((right - notch, scene_top));
            marker.line_to((right, scene_top));
            marker.line_to((right, scene_top + notch));
            marker.close();
            canvas.draw_path(&marker.detach(), &Ink::paint(color));
            if right - rail > 12. {
                let ink = if scene.frames.contains(&state.frame) {
                    TEXT
                } else {
                    QUIET
                };
                ui.clipped_text(
                    &scene.name,
                    Rect::from_ltrb(rail + 3., scene_top, right - 2., strip_top),
                    scene_top + 16.,
                    11.,
                    ink,
                    false,
                );
            }
        }
        if self.film_height > 0. && !model.thumbnails.is_empty() {
            let strip = Rect::from_ltrb(
                track.left,
                strip_top,
                track.right,
                strip_top + self.film_height,
            );
            canvas.save();
            canvas.clip_rrect(RRect::new_rect_xy(strip, 5., 5.), None, true);
            ui.fill(strip, CANVAS);
            let w = strip.width() / model.thumbnails.len() as f32;
            for (slot, image) in model.thumbnails.iter().enumerate() {
                let Some(image) = image else {
                    continue;
                };
                let rect =
                    Rect::from_xywh(track.left + slot as f32 * w, strip.top, w, self.film_height);
                ui.thumbnail(image, rect);
            }
            canvas.restore();
        }
        let first_lane = scroll
            .audio
            .min(model.audio_rows().saturating_sub(self.audio_slots));
        for (row, lane) in model
            .audio
            .iter()
            .skip(first_lane)
            .take(self.audio_slots)
            .enumerate()
        {
            let top = strip_top + self.film_height + 6. + LANE * row as f32;
            let label = if model.audio_rows() > self.audio_slots {
                format!("A{}/{}", row + first_lane + 1, model.audio_rows())
            } else {
                format!("A{}", row + first_lane + 1)
            };
            ui.text(&label, MARGIN, top + 14., 12., QUIET, false);
            for clip in lane {
                let r = Rect::from_ltrb(
                    self.frame_position(clip.frames.start, duration),
                    top,
                    self.frame_position(clip.frames.end, duration),
                    top + LANE - 3.,
                );
                ui.round(r, AUDIO, 4.);
                if let Some(path) = &clip.waveform {
                    canvas.save();
                    canvas.translate((r.left, r.top + 2.));
                    canvas.scale((r.width(), r.height() - 3.));
                    canvas.draw_path(path, &Ink::paint(Color::from_rgb(86, 86, 86)));
                    canvas.restore();
                }
                if r.width() > 20. {
                    let label = Rect::from_ltrb(r.left + 5., r.top, r.right - 2., r.bottom);
                    ui.clipped_text(&clip.name, label, r.top + 13., 11., TEXT, true);
                }
            }
        }
    }

    fn draw_playhead(&self, ui: &Ink<'_>, state: &ControlsState) {
        let track = self.tracks();
        if self.portrait {
            let film = self.portrait_columns().0;
            let y = self.frame_position(state.frame, state.duration_in_frames);
            ui.fill(
                Rect::from_ltrb(film.left - 2., y - 1., track.right, y + 1.),
                ORANGE,
            );
            ui.round(Rect::from_xywh(film.left - 4., y - 4., 7., 8.), ORANGE, 2.);
            return;
        }
        let x = self.frame_position(state.frame, state.duration_in_frames);
        ui.fill(
            Rect::from_ltrb(x - 1., track.top, x + 1., track.bottom),
            ORANGE,
        );
        ui.round(Rect::from_xywh(x - 4., track.top - 3., 8., 7.), ORANGE, 2.);
    }
}

fn tick_step(duration: usize, fps: usize, span: f32, spacing: f32) -> usize {
    let fps = fps.max(1);
    let unit = if duration < fps { 1 } else { fps };
    let ticks = (span / spacing).floor().max(1.) as usize;
    let target = duration.div_ceil(unit).div_ceil(ticks).max(1);
    let power = 10_usize.pow(target.ilog10());
    [1_usize, 2, 5, 10]
        .into_iter()
        .map(|m| power.saturating_mul(m))
        .find(|&step| step >= target)
        .unwrap()
        .saturating_mul(unit)
}

#[derive(Clone)]
struct Ink<'a> {
    canvas: &'a Canvas,
    font: Font,
    medium: Font,
}

impl<'a> Ink<'a> {
    fn new(canvas: &'a Canvas, controls: &Controls) -> Self {
        Self {
            canvas,
            font: controls.font(false, 1.),
            medium: controls.font(true, 1.),
        }
    }

    fn paint(color: Color) -> Paint {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(color);
        paint
    }

    fn thumbnail(&self, image: &Image, rect: Rect) {
        let canvas = self.canvas;
        let scale =
            (rect.width() / image.width() as f32).max(rect.height() / image.height() as f32);
        let mut dst = Rect::from_wh(image.width() as f32 * scale, image.height() as f32 * scale);
        dst.offset((
            rect.center_x() - dst.center_x(),
            rect.center_y() - dst.center_y(),
        ));
        canvas.save();
        canvas.clip_rect(rect, None, false);
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            dst,
            SamplingOptions::from(FilterMode::Linear),
            &Paint::default(),
        );
        canvas.restore();
    }

    fn fill(&self, rect: Rect, color: Color) {
        self.canvas.draw_rect(rect, &Self::paint(color));
    }

    fn round(&self, rect: Rect, color: Color, radius: f32) {
        self.canvas
            .draw_round_rect(rect, radius, radius, &Self::paint(color));
    }

    fn text(&self, text: &str, x: f32, y: f32, size: f32, color: Color, medium: bool) {
        let font = if medium { &self.medium } else { &self.font };
        self.canvas.draw_str(
            text,
            (x, y),
            &font.with_size(size).unwrap(),
            &Self::paint(color),
        );
    }

    fn clipped_text(
        &self,
        text: &str,
        rect: Rect,
        baseline: f32,
        size: f32,
        color: Color,
        medium: bool,
    ) {
        self.canvas.save();
        self.canvas.clip_rect(rect, None, false);
        self.text(text, rect.left, baseline, size, color, medium);
        self.canvas.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fframes::TimelineScene;

    fn studio() -> Studio {
        Studio {
            video_aspect: 16. / 9.,
            index: TimelineIndex {
                fps: 30,
                duration_in_frames: 100,
                scenes: [0..60, 50..100, 110..115]
                    .into_iter()
                    .enumerate()
                    .map(|(index, frames)| TimelineScene {
                        index,
                        frames,
                        name: format!("Scene {index}"),
                        full_name: String::new(),
                    })
                    .collect(),
            },
            audio: vec![],
            thumbnails: vec![],
            thumbnail_arrivals: vec![],
            thumbnail_revision: 0,
        }
    }

    #[test]
    fn navigation_uses_resolved_starts_and_skips_unreachable_scenes() {
        let studio = studio();
        assert_eq!(studio.adjacent_scene(49, true), Some(50));
        assert_eq!(studio.adjacent_scene(50, false), Some(0));
        assert_eq!(studio.adjacent_scene(51, false), Some(50));
        assert_eq!(studio.adjacent_scene(50, true), None);
        assert_eq!(studio.adjacent_scene(0, false), None);
    }

    #[test]
    fn cached_timeline_matches_fresh_draws_after_scene_scroll_thumbnail_and_scale_changes() {
        let controls = Controls::new();
        let mut model = studio();
        model.thumbnails = vec![None];
        model.thumbnail_arrivals = vec![None];
        model.audio = (0..4)
            .map(|i| {
                vec![Clip {
                    name: format!("Audio {i}"),
                    frames: i * 10..100,
                    waveform: None,
                }]
            })
            .collect();
        let mut scroll = StudioScroll::default();
        let mut state = ControlsState {
            frame: 0,
            duration_in_frames: 100,
            fps: 30,
            fullscreen: false,
            motion: crate::motion::Motion::new(
                crate::motion::Intent {
                    studio: true,
                    dock: false,
                    playing: true,
                    looping: true,
                    hovered: None,
                    pressed: None,
                },
                crate::motion::Timing::DEFAULT,
                Instant::now(),
            )
            .sample(Instant::now()),
        };
        let draw = |layout: &mut StudioLayout,
                    state: &ControlsState,
                    model: &Studio,
                    scroll: &StudioScroll,
                    scale: f32,
                    cached: bool| {
            let size = (800. * scale) as i32;
            let info = ImageInfo::new_n32_premul((size, size), None);
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::BLACK);
            canvas.scale((scale, scale));
            if cached {
                layout.draw(canvas, &controls, state, model, scroll);
            } else {
                let ui = Ink::new(canvas, &controls);
                layout.draw_tracks(&ui, state, model, scroll);
                layout.draw_transport(&ui, &controls, state);
                layout.draw_playhead(&ui, state);
            }
            let mut pixels = vec![0; (size * size * 4) as usize];
            assert!(surface.read_pixels(&info, &mut pixels, size as usize * 4, (0, 0)));
            pixels
        };
        for aspect in [16. / 9., 9. / 16.] {
            let mut layout = StudioLayout::new(800., 800., 4, aspect).unwrap();
            for scale in [1., 1.25, 2.] {
                let mut first_id = None;
                for frame in [0, 1, 49, 50, 59, 60, 99, 0] {
                    state.frame = frame;
                    assert!(
                        draw(&mut layout, &state, &model, &scroll, scale, true)
                            == draw(&mut layout, &state, &model, &scroll, scale, false),
                        "aspect={aspect}, frame={frame}, scale={scale}"
                    );
                    let id = layout.timeline_cache.as_ref().unwrap().image.unique_id();
                    if frame == 0 {
                        first_id = Some(id);
                    } else if frame == 1 {
                        assert_eq!(first_id, Some(id));
                    }
                }
            }
            scroll.audio = 1;
            let mut thumbnail = surfaces::raster_n32_premul((20, 10)).unwrap();
            thumbnail.canvas().clear(Color::RED);
            for updated_thumbnail in [false, true] {
                if updated_thumbnail {
                    // arrived long ago: its fade-in is over, so the cached and the fresh
                    // draw show it the same way
                    let arrival = Instant::now().checked_sub(Duration::from_secs(60)).unwrap();
                    model.set_thumbnail(0, thumbnail.image_snapshot(), arrival);
                }
                assert!(
                    draw(&mut layout, &state, &model, &scroll, 2., true)
                        == draw(&mut layout, &state, &model, &scroll, 2., false),
                    "aspect={aspect}, updated_thumbnail={updated_thumbnail}"
                );
            }
            // The panel slides while the views change: a moved canvas gets a fresh cache
            scroll.audio = 0;
            model.thumbnails[0] = None;
        }
    }

    #[test]
    fn timeline_seek_and_hit_regions_agree_across_sizes() {
        let model = studio();
        for width in [320., 440., 460., 560., 1280.] {
            let layout = StudioLayout::new(width, 700., 2, model.video_aspect).unwrap();
            let track = layout.tracks();
            assert_eq!(layout.hit(track.center(), &model), Some(Control::Timeline));
            assert_eq!(layout.hit(layout.stage.center(), &model), None);
            assert_eq!(layout.stage.top, 0.);
            let buttons = layout.buttons();
            assert_eq!(buttons[1].1.center_x(), width / 2.);
            assert!(
                buttons
                    .windows(2)
                    .all(|pair| pair[0].1.right <= pair[1].1.left)
            );
            assert!(layout.focus_hit(layout.restore.center()));
            assert_eq!(layout.hit(layout.restore.center(), &model), None);
            assert_eq!(layout.frame_at(track.left - 1., 100), 0);
            assert_eq!(layout.frame_at(track.right, 100), 99);
            assert_eq!(layout.frame_at(track.center_x(), 100), 50);
            assert_eq!(layout.frame_at(track.right, 1), 0);
            assert_eq!(layout.frame_at(track.right, 0), 0);
            let scene = Point::new(track.center_x() + 1., track.top + RULER + SCENE_HEIGHT / 2.);
            assert_eq!(layout.hit(scene, &model), Some(Control::Scene(50)));
            assert_eq!(layout.stage.left, 0.);
            assert!(track.bottom < 700.);
            for (control, rect) in layout.buttons() {
                assert_eq!(layout.hit(rect.center(), &model), Some(control));
            }
        }
        let compact = StudioLayout::new(320., 240., 2, model.video_aspect).unwrap();
        assert_eq!(compact.timeline.bottom, 240.);
        assert_eq!(compact.audio_slots, 0);
        assert!(StudioLayout::new(300., 200., 0, model.video_aspect).is_none());
    }

    #[test]
    fn portrait_uses_the_left_timeline_for_scenes_and_vertical_seeking() {
        let model = studio();
        for (width, height) in [(765., 720.), (1280., 720.), (405., 720.), (320., 240.)] {
            let ui = StudioLayout::new(width, height, 2, 9. / 16.).unwrap();
            let track = ui.tracks();
            let (film, scenes, audio) = ui.portrait_columns();
            assert!(ui.portrait);
            assert_eq!(ui.stage.top, 0.);
            assert_eq!(ui.stage.left, ui.timeline.right);
            assert!(
                track.right < ui.stage.left
                    && film.right < scenes.left
                    && scenes.right < audio.left
            );
            assert_eq!(ui.hit(ui.stage.center(), &model), None);
            assert_eq!(ui.frame_at(ui.time_axis(film.center()), 100), 50);
            assert_eq!(ui.frame_at(track.top - 10., 100), 0);
            assert_eq!(ui.frame_at(track.bottom + 10., 100), 99);
            assert_eq!(ui.hit(film.center(), &model), Some(Control::Timeline));
            assert_eq!(ui.hit(scenes.center(), &model), Some(Control::Scene(50)));
            assert_eq!(ui.buttons()[1].1.center_x(), ui.transport.center_x());
            for (control, button) in ui.buttons() {
                assert_eq!(ui.hit(button.center(), &model), Some(control));
            }
            if width == 765. {
                assert_eq!(ui.stage, Rect::from_ltrb(360., 0., 765., 720.));
            }
        }
        assert!(!StudioLayout::new(405., 720., 2, 16. / 9.).unwrap().portrait);
        assert!(!StudioLayout::new(405., 720., 2, 1.).unwrap().portrait);
    }

    #[test]
    fn waveform_uses_both_channels_and_the_selected_source_interval() {
        let data = PreloadedAudioData {
            sample_rate: 512,
            samples: vec![0.; 1024].into(),
            right: Some([vec![0.; 512], vec![0.75; 512]].concat().into()),
        };
        assert_eq!(waveform(&data, 0., 1.).bounds().top, 1.);
        assert_eq!(waveform(&data, 1., 1.).bounds().top, 0.25);
        assert_eq!(waveform(&data, 2., 1.).bounds().top, 1.);
    }

    #[test]
    fn overlapping_audio_stacks_and_reuses_the_first_free_lane() {
        let clips = [15..18, 0..32, 3..5, 5..7].map(|frames| Clip {
            name: String::new(),
            waveform: None,
            frames,
        });
        let lanes = pack_clips(Vec::from(clips));
        assert_eq!(lanes.len(), 2);
        assert_eq!(lanes[0][0].frames, 0..32);
        assert_eq!(
            lanes[1]
                .iter()
                .map(|clip| clip.frames.clone())
                .collect::<Vec<_>>(),
            [3..5, 5..7, 15..18]
        );
    }
}
