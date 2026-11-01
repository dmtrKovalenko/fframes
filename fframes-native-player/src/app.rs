use std::fmt::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowId};

use crate::controls::{Control, ControlsState, format_time};
use crate::error::PlayerError;
use crate::options::PlayerBackend;
use crate::presenter::Presenter;
use crate::scheduler::Scheduler;
use crate::studio::{Studio, StudioLayout, StudioScroll};
use fframes_skia_renderer::skia_safe::Point;

/// The control bar hides after this long without pointer movement while playing.
const CONTROLS_IDLE_TIMEOUT: Duration = Duration::from_millis(2500);

/// A frame worker finished a playback or filmstrip job.
pub(crate) enum FrameReady {
    Playback,
    Failed { position: u64, epoch: u64 },
    Thumbnail(usize, fframes_skia_renderer::skia_safe::Image),
}

/// Playback clock in player positions (see `scheduler`), driven by wall time.
struct Clock {
    fps: f64,
    anchor_position: f64,
    /// `Some` while playing.
    anchor_instant: Option<Instant>,
}

impl Clock {
    fn position(&self) -> u64 {
        self.exact_position() as u64
    }

    fn exact_position(&self) -> f64 {
        match self.anchor_instant {
            Some(instant) => self.anchor_position + instant.elapsed().as_secs_f64() * self.fps,
            None => self.anchor_position,
        }
    }

    fn is_playing(&self) -> bool {
        self.anchor_instant.is_some()
    }

    fn set(&mut self, position: u64, playing: bool) {
        self.anchor_position = position as f64;
        self.anchor_instant = playing.then(Instant::now);
    }

    /// When the next frame is due.
    fn next_frame_at(&self) -> Option<Instant> {
        let instant = self.anchor_instant?;
        let next = (self.exact_position().floor() + 1.) - self.anchor_position;
        Some(instant + Duration::from_secs_f64(next / self.fps))
    }
}

pub(crate) struct AppConfig<'a> {
    pub studio: Studio,
    pub title: &'a str,
    pub window_size: LogicalSize<u32>,
    pub fit_portrait_to_screen: bool,
    pub backend: PlayerBackend,
    pub vsync: bool,
    pub fps: usize,
    pub duration_in_frames: usize,
    pub background: fframes::Color,
    pub looping: bool,
    pub autoplay: bool,
    pub start_frame: usize,
}

pub(crate) struct App<'s> {
    config: AppConfig<'s>,
    scheduler: &'s Scheduler,
    #[cfg(feature = "audio")]
    audio: Option<&'s crate::audio::AudioOutput>,

    window: Option<Rc<Window>>,
    /// The tree on screen. Skia copies its video frames to the GPU during the flush.
    current: Option<(u64, fframes::usvgr::Tree)>,
    presenter: Option<Presenter>,
    redraw_pending: bool,
    pub(crate) error: Option<PlayerError>,

    clock: Clock,
    looping: bool,
    /// Toggled with `b`.
    controls_enabled: bool,
    controls_visible: bool,
    needs_redraw: bool,

    cursor: PhysicalPosition<f64>,
    cursor_inside: bool,
    last_pointer_activity: Instant,
    hovered: Option<Control>,
    drag: Option<Control>,
    scene_press_axis: f32,
    /// The newest destination under the pointer while a native frame is being prepared.
    scrub_target: Option<usize>,
    studio_open: bool,
    scroll: StudioScroll,

    stats_since: Instant,
    /// New frames drawn into the window since `stats_since`. Frames taken from the
    /// scheduler can be replaced before a redraw, so they are not counted.
    frames_shown: usize,
    /// The position of the frame last drawn into the window.
    presented: Option<u64>,
    shown_fps: f64,
    title_updated_at: Instant,
}

impl<'s> App<'s> {
    pub(crate) fn new(
        config: AppConfig<'s>,
        scheduler: &'s Scheduler,
        #[cfg(feature = "audio")] audio: Option<&'s crate::audio::AudioOutput>,
    ) -> Self {
        let now = Instant::now();
        let clock = Clock {
            fps: config.fps as f64,
            anchor_position: 0.,
            anchor_instant: None,
        };

        let mut app = Self {
            looping: config.looping,
            scheduler,
            #[cfg(feature = "audio")]
            audio,
            window: None,
            presenter: None,
            redraw_pending: false,
            error: None,
            clock,
            current: None,
            controls_enabled: true,
            controls_visible: true,
            needs_redraw: true,
            cursor: PhysicalPosition::default(),
            cursor_inside: false,
            last_pointer_activity: now,
            hovered: None,
            drag: None,
            scene_press_axis: 0.,
            scrub_target: None,
            studio_open: true,
            scroll: StudioScroll::default(),
            stats_since: now,
            frames_shown: 0,
            presented: None,
            shown_fps: 0.,
            title_updated_at: now,
            config,
        };

        let start = app.config.start_frame.min(app.last_frame()) as u64;
        app.seek_to(start, app.config.autoplay);
        app
    }

    fn last_frame(&self) -> usize {
        self.config.duration_in_frames.saturating_sub(1)
    }

    fn current_frame(&self) -> usize {
        let position = self
            .current
            .as_ref()
            .map_or_else(|| self.clock.position(), |(position, _)| *position);

        self.scheduler.frame_of(position, self.looping)
    }

    /// Seeks to `position`, reusing prepared scenes when possible.
    fn seek_to(&mut self, position: u64, playing: bool) {
        self.scrub_target = None;
        if !self.scheduler.seek_buffered(position, self.looping) {
            self.scheduler.seek(position, self.looping);
        }
        self.set_playback(position, playing);
    }

    fn set_playback(&mut self, position: u64, playing: bool) {
        self.clock.set(position, playing);
        #[cfg(feature = "audio")]
        if let Some(audio) = self.audio {
            audio.reset(position, self.config.fps, playing, self.looping);
        }
    }

    fn playhead(&self) -> usize {
        self.scrub_target.unwrap_or_else(|| self.current_frame())
    }

    fn seek_to_frame(&mut self, frame: i64) {
        let frame = frame.clamp(0, self.last_frame() as i64) as u64;
        self.seek_to(frame, self.clock.is_playing());
    }

    fn toggle_play(&mut self) {
        let frame = self.playhead();
        let playing = !self.clock.is_playing();
        let position = self.clock.position();
        if playing && !self.looping && frame >= self.last_frame() {
            self.seek_to(0, true);
        } else if self.scrub_target.is_none()
            && self.scheduler.frame_of(position, self.looping) == frame
        {
            // The workers have already prepared frames ahead of this position.
            self.set_playback(position, playing);
        } else {
            self.seek_to(frame as u64, playing);
        }
    }

    /// Advances the clock and picks up the frame that should be on screen now.
    fn tick(&mut self) {
        let mut position = self.clock.position();

        if !self.looping && position >= self.last_frame() as u64 {
            position = self.last_frame() as u64;
            if self.clock.is_playing() {
                self.clock.set(position, false);
            }
        }

        if let Some(frame) = self.scheduler.take(position) {
            let next_frame = self.scheduler.frame_of(frame.0, self.looping);
            self.current = Some(frame);
            self.needs_redraw = true;
            self.finish_scrub(next_frame);
        }

        let controls_visible = self.controls_should_be_visible();
        if controls_visible != self.controls_visible {
            self.controls_visible = controls_visible;
            self.needs_redraw = true;
        }

        if self.needs_redraw {
            self.needs_redraw = false;
            if let Some(window) = &self.window {
                self.redraw_pending = true;
                window.request_redraw();
            }
        }

        self.update_title();
    }

    fn finish_scrub(&mut self, frame: usize) {
        if let Some(target) = self.scrub_target.take() {
            if target != frame {
                self.seek_to(target as u64, false);
                self.scrub_target = Some(target);
            }
            self.needs_redraw = true;
        }
    }

    fn frame_ready(&mut self, event: FrameReady) {
        match event {
            FrameReady::Thumbnail(slot, image) => {
                self.config.studio.set_thumbnail(slot, image);
                self.needs_redraw = true;
            }
            FrameReady::Failed { position, epoch }
                if self.scheduler.is_current_seek(position, epoch) =>
            {
                self.finish_scrub(self.scheduler.frame_of(position, self.looping));
            }
            FrameReady::Playback | FrameReady::Failed { .. } => {}
        }
    }

    fn update_title(&mut self) {
        let now = Instant::now();
        let stats_elapsed = now.duration_since(self.stats_since).as_secs_f64();
        if stats_elapsed >= 1. {
            self.shown_fps = self.frames_shown as f64 / stats_elapsed;
            self.frames_shown = 0;
            self.stats_since = now;
        }

        if now.duration_since(self.title_updated_at) < Duration::from_millis(250) {
            return;
        }
        self.title_updated_at = now;

        let Some(window) = &self.window else { return };
        let fps = self.config.fps;
        let frame = self.current_frame();
        let mut title = format!(
            "{} | {} / {} | frame {}/{}",
            self.config.title,
            format_time(frame, fps),
            format_time(self.config.duration_in_frames, fps),
            frame,
            self.last_frame(),
        );

        if self.clock.is_playing() {
            let _ = write!(title, " | {:.1}/{fps} fps", self.shown_fps);
        } else {
            title += " | paused";
        }
        if self.looping {
            title += " | loop";
        }
        if self.presenter.as_ref().is_some_and(|p| !p.is_gpu()) {
            title += " | cpu";
        }

        window.set_title(&title);
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }

        let frame = self.playhead() as i64;
        let second = self.config.fps as i64;
        match event.logical_key.as_ref() {
            Key::Named(NamedKey::Space) => self.toggle_play(),
            Key::Named(NamedKey::Escape) | Key::Character("q") => event_loop.exit(),
            Key::Named(NamedKey::ArrowLeft) | Key::Character("h") => {
                self.seek_to_frame(frame - second);
            }
            Key::Named(NamedKey::ArrowRight) | Key::Character("l") => {
                self.seek_to_frame(frame + second);
            }
            Key::Named(NamedKey::ArrowDown) | Key::Character("j" | ",") => self.step(frame - 1),
            Key::Named(NamedKey::ArrowUp) | Key::Character("k" | ".") => self.step(frame + 1),
            Key::Named(NamedKey::Home) | Key::Character("g") => self.seek_to_frame(0),
            Key::Named(NamedKey::End) | Key::Character("G") => {
                self.seek_to_frame(self.last_frame() as i64);
            }
            Key::Character("r") => {
                self.looping = !self.looping;
                let playing = self.clock.is_playing();
                self.seek_to(frame as u64, playing);
            }
            Key::Character("b") => {
                self.controls_enabled = !self.controls_enabled;
                self.needs_redraw = true;
            }
            Key::Character("t") => self.toggle_studio(),
            Key::Character("[" | "]") => {
                let next = event.logical_key == Key::Character("]".into());
                if let Some(target) = self.config.studio.adjacent_scene(frame as usize, next) {
                    self.seek_to_frame(target as i64);
                }
            }
            Key::Character("f") => self.toggle_fullscreen(),
            Key::Character(digit) if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
                let tenth = i64::from(digit.as_bytes()[0] - b'0');
                self.seek_to_frame(self.config.duration_in_frames as i64 * tenth / 10);
            }
            _ => {}
        }
    }

    /// Frame stepping always pauses.
    fn step(&mut self, frame: i64) {
        let frame = frame.clamp(0, self.last_frame() as i64) as u64;
        self.seek_to(frame, false);
    }

    fn studio_layout(&self) -> Option<&StudioLayout> {
        self.presenter.as_ref()?.studio_layout.as_ref()
    }

    fn pointer(&self) -> Point {
        let scale = self.window.as_ref().map_or(1., |w| w.scale_factor());
        let pos = self.cursor.to_logical::<f32>(scale);
        Point::new(pos.x, pos.y)
    }

    fn invalidate_view(&mut self) {
        self.needs_redraw = true;
    }

    /// A drag along the timeline or the seek slider turns lookahead off: the workers
    /// prepare only the frame under the pointer until the button is released.
    fn set_drag(&mut self, drag: Option<Control>) {
        self.drag = drag;
        self.scheduler
            .set_scrubbing(matches!(drag, Some(Control::Seek | Control::Timeline)));
    }

    fn toggle_studio(&mut self) {
        self.invalidate_view();
        self.studio_open = !self.studio_open;
        self.set_drag(None);
        self.hovered = None;
        self.last_pointer_activity = Instant::now();
        self.needs_redraw = true;
    }

    fn toggle_fullscreen(&mut self) {
        let Some(window) = &self.window else { return };
        let fullscreen = match window.fullscreen() {
            Some(_) => None,
            None => Some(Fullscreen::Borderless(None)),
        };
        window.set_fullscreen(fullscreen);
    }

    fn is_fullscreen(&self) -> bool {
        self.window
            .as_ref()
            .is_some_and(|window| window.fullscreen().is_some())
    }

    /// Always shown while paused, while playing only after recent pointer activity.
    fn controls_should_be_visible(&self) -> bool {
        self.controls_enabled
            && (!self.clock.is_playing()
                || self.drag.is_some()
                || (self.cursor_inside && self.hovered.is_some())
                || self.last_pointer_activity.elapsed() < CONTROLS_IDLE_TIMEOUT)
    }

    fn control_under_cursor(&self) -> Option<Control> {
        let point = self.pointer();
        if self.studio_open
            && let Some(layout) = self.studio_layout()
        {
            return layout.hit(point, &self.config.studio);
        }
        if self.controls_visible
            && let Some(control) = self
                .presenter
                .as_ref()?
                .controls_layout()
                .hit(self.cursor.x as f32, self.cursor.y as f32)
        {
            return Some(control);
        }
        let layout = self.studio_layout()?;
        (self.controls_visible && layout.focus_hit(point)).then_some(Control::Focus)
    }

    fn seek_to_cursor(&mut self) {
        let Some(presenter) = &self.presenter else {
            return;
        };
        if self.drag == Some(Control::Timeline) {
            if let Some(ui) = self.studio_layout() {
                let frame =
                    ui.frame_at(ui.time_axis(self.pointer()), self.config.duration_in_frames);
                // Finish the in-flight frame before requesting the newest pointer
                // position. Starting a fresh generation on every move can starve it.
                if self.scrub_target.is_none() {
                    if !self.clock.is_playing()
                        && self.current.is_some()
                        && frame == self.current_frame()
                        && self.clock.position() == frame as u64
                    {
                        return;
                    }
                    self.seek_to(frame as u64, false);
                }
                self.scrub_target = Some(frame);
                self.needs_redraw = true;
            }
            return;
        }
        let progress = f64::from(
            presenter
                .controls_layout()
                .progress_at(self.cursor.x as f32),
        );
        let frame = (progress * self.last_frame() as f64).round() as i64;
        self.seek_to_frame(frame);
    }

    fn on_pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        self.cursor = position;
        self.cursor_inside = true;
        self.last_pointer_activity = Instant::now();

        if matches!(self.drag, Some(Control::Scene(_)))
            && self.studio_layout().is_some_and(|ui| {
                (ui.time_axis(self.pointer()) - self.scene_press_axis).abs() >= 3.
            })
        {
            self.set_drag(Some(Control::Timeline));
        }
        if matches!(self.drag, Some(Control::Seek | Control::Timeline)) {
            self.seek_to_cursor();
        }

        let hovered = self.control_under_cursor();
        if hovered != self.hovered {
            self.hovered = hovered;
            self.needs_redraw = true;

            if let Some(window) = &self.window {
                window.set_cursor(match hovered {
                    None | Some(Control::Bar) => CursorIcon::Default,
                    Some(Control::Timeline)
                        if self.studio_layout().is_some_and(|ui| ui.portrait) =>
                    {
                        CursorIcon::NsResize
                    }
                    Some(Control::Timeline) => CursorIcon::EwResize,
                    Some(_) => CursorIcon::Pointer,
                });
            }
        }

        // Bring the controls back without waiting for the next frame.
        if !self.controls_visible && self.controls_should_be_visible() {
            self.controls_visible = true;
            self.needs_redraw = true;
        }
        if self.needs_redraw
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }

    fn on_pointer_pressed(&mut self) {
        let frame = self.playhead() as i64;
        let second = self.config.fps as i64;

        match self.control_under_cursor() {
            Some(Control::Back) => self.seek_to_frame(frame - second),
            Some(Control::Forward) => self.seek_to_frame(frame + second),
            Some(control @ (Control::Seek | Control::Timeline)) => {
                self.set_drag(Some(control));
                self.seek_to_cursor();
            }
            Some(control @ Control::Scene(_)) => {
                if let Some(ui) = self.studio_layout() {
                    self.scene_press_axis = ui.time_axis(self.pointer());
                }
                self.set_drag(Some(control));
            }
            Some(Control::Focus) => self.toggle_studio(),
            Some(Control::Loop) => {
                self.looping = !self.looping;
                let playing = self.clock.is_playing();
                self.seek_to(frame as u64, playing);
            }
            Some(Control::Fullscreen) => self.toggle_fullscreen(),
            Some(Control::Bar) => {}
            // Clicking the video itself toggles playback too
            Some(Control::PlayPause) | None => self.toggle_play(),
        }
        self.needs_redraw = true;
    }

    fn on_pointer_released(&mut self) {
        let drag = self.drag;
        self.set_drag(None);
        if let Some(control @ Control::Scene(start)) = drag
            && self.control_under_cursor() == Some(control)
        {
            self.step(start as i64);
        }
    }

    fn redraw(&mut self) -> Result<(), PlayerError> {
        self.redraw_pending = false;
        let controls = ControlsState {
            visible: self.controls_visible,
            frame: self.playhead(),
            duration_in_frames: self.config.duration_in_frames,
            fps: self.config.fps,
            playing: self.clock.is_playing(),
            looping: self.looping,
            fullscreen: self.is_fullscreen(),
            hovered: self.hovered,
        };

        if let Some(presenter) = self.presenter.as_mut() {
            presenter.present(
                self.current.as_ref().map(|(_, tree)| tree),
                &controls,
                &self.config.studio,
                &self.scroll,
                self.studio_open,
            )?;

            let position = self.current.as_ref().map(|(position, _)| *position);
            if position.is_some() && position != self.presented {
                self.frames_shown += 1;
            }
            self.presented = position;
        }

        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: PlayerError) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler<FrameReady> for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        if self.config.fit_portrait_to_screen
            && let Some(monitor) = event_loop
                .primary_monitor()
                .or_else(|| event_loop.available_monitors().next())
        {
            let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
            // Leave room for the window decorations and desktop controls.
            let height = f64::from(self.config.window_size.height)
                .min(screen.height - 72.)
                .max(1.);
            let width = height * f64::from(self.config.studio.video_aspect)
                + f64::from(crate::studio::PORTRAIT_SIDEBAR_WIDTH);
            self.config.window_size = LogicalSize::new(
                width.min(screen.width - 24.).max(1.).round() as u32,
                height.round() as u32,
            );
        }

        let attributes = Window::default_attributes()
            .with_title(self.config.title)
            .with_inner_size(self.config.window_size);

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => return self.fail(event_loop, err.into()),
        };

        match Presenter::new(window.clone(), &self.config) {
            Ok(presenter) => self.presenter = Some(presenter),
            Err(err) => return self.fail(event_loop, err),
        }

        self.window = Some(window);
        self.needs_redraw = true;
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: FrameReady) {
        self.frame_ready(event);
        self.tick();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(Err(err)) = self.presenter.as_mut().map(Presenter::resize) {
                    return self.fail(event_loop, err);
                }
                self.invalidate_view();
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.redraw() {
                    self.fail(event_loop, err);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, event),
            WindowEvent::CursorMoved { position, .. } => self.on_pointer_moved(position),
            WindowEvent::MouseWheel { delta, .. } if self.studio_open && self.cursor_inside => {
                let point = self.pointer();
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, dy) => f64::from(dy),
                    MouseScrollDelta::PixelDelta(delta) => delta.y,
                };
                if let Some(ui) = self
                    .presenter
                    .as_ref()
                    .and_then(|p| p.studio_layout.as_ref())
                {
                    ui.scroll(
                        &self.config.studio,
                        &mut self.scroll,
                        point,
                        isize::from(dy < 0.) - isize::from(dy > 0.),
                    );
                    self.hovered = self.control_under_cursor();
                    self.needs_redraw = true;
                }
            }
            WindowEvent::Focused(false) => self.set_drag(None),
            WindowEvent::CursorLeft { .. } => {
                self.cursor_inside = false;
                if self.hovered.take().is_some() {
                    self.needs_redraw = true;
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.on_pointer_pressed(),
                ElementState::Released => self.on_pointer_released(),
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.tick();

        event_loop.set_control_flow(match self.clock.next_frame_at() {
            Some(next_frame) => ControlFlow::WaitUntil(next_frame),
            // Paused: sleep until input or a worker delivers the requested frame.
            None => ControlFlow::Wait,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paused_app(scheduler: &Scheduler) -> App<'_> {
        let timeline = fframes::ResolvedRenderingTimeline {
            audio_map: None,
            scenes: None,
            duration_in_frames: 100,
        };
        let time = fframes::TimeBase {
            fps: 30,
            sample_rate: 44100,
        };
        App::new(
            AppConfig {
                studio: Studio::new(time, &timeline, None, 0, 1.),
                title: "test",
                window_size: LogicalSize::new(640, 480),
                fit_portrait_to_screen: false,
                backend: PlayerBackend::Cpu,
                vsync: false,
                fps: time.fps,
                duration_in_frames: timeline.duration_in_frames,
                background: fframes::Color::BLACK,
                looping: false,
                autoplay: false,
                start_frame: 0,
            },
            scheduler,
            #[cfg(feature = "audio")]
            None,
        )
    }

    #[test]
    fn resuming_keeps_prefetched_frames_unless_playback_restarts() {
        let scheduler = Scheduler::new(100, 3, false, 0, false);
        let mut app = paused_app(&scheduler);
        app.seek_to(10, false);
        app.toggle_play();
        assert!(app.clock.is_playing());
        assert!(scheduler.is_current_seek(10, 2));

        app.seek_to(99, false);
        app.toggle_play();
        assert!(scheduler.is_current_seek(0, 4));
    }

    #[test]
    fn failed_scrub_continues_at_the_latest_pointer_position() {
        let scheduler = Scheduler::new(100, 3, false, 0, false);
        let mut app = paused_app(&scheduler);
        app.seek_to(10, false);
        app.scrub_target = Some(20);
        // A stale completion (even at the same position) and a failed lookahead
        // frame must not advance the pending scrub.
        for (position, epoch) in [(10, 1), (11, 2)] {
            app.frame_ready(FrameReady::Failed { position, epoch });
            assert_eq!(app.clock.position(), 10);
            assert_eq!(app.scrub_target, Some(20));
        }
        app.frame_ready(FrameReady::Failed {
            position: 10,
            epoch: 2,
        });
        assert_eq!(app.clock.position(), 20);
        assert_eq!(app.scrub_target, Some(20));
        app.frame_ready(FrameReady::Failed {
            position: 20,
            epoch: 3,
        });
        assert_eq!(app.scrub_target, None);
        assert!(scheduler.is_current_seek(20, 3));
    }

    #[test]
    fn a_timeline_drag_turns_lookahead_off_until_the_button_is_released() {
        let scheduler = Scheduler::new(100, 3, false, 0, false);
        let mut app = paused_app(&scheduler);
        app.set_drag(Some(Control::Scene(0)));
        assert!(!scheduler.is_scrubbing());
        app.set_drag(Some(Control::Timeline));
        assert!(scheduler.is_scrubbing());
        app.on_pointer_released();
        assert!(!scheduler.is_scrubbing());
        assert_eq!(app.drag, None);
    }
}
