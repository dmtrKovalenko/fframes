use std::rc::Rc;
use std::time::{Duration, Instant};

use fframes::usvgr;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::error::PlayerError;
use crate::options::PlayerBackend;
use crate::presenter::{Overlay, Presenter, SEEK_BAR_HIT_HEIGHT};
use crate::scheduler::Scheduler;

/// Sent by the frame workers whenever a frame becomes available.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameReady;

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
    pub title: &'a str,
    pub window_size: LogicalSize<u32>,
    pub backend: PlayerBackend,
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
    presenter: Option<Presenter>,
    pub(crate) error: Option<PlayerError>,

    clock: Clock,
    looping: bool,
    current: Option<(u64, usvgr::Tree)>,
    show_overlay: bool,
    needs_redraw: bool,

    cursor: PhysicalPosition<f64>,
    seeking_with_mouse: bool,

    stats_since: Instant,
    frames_shown: usize,
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
            error: None,
            clock,
            current: None,
            show_overlay: true,
            needs_redraw: true,
            cursor: PhysicalPosition::default(),
            seeking_with_mouse: false,
            stats_since: now,
            frames_shown: 0,
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

    /// Restarts playback at `position`, all the prepared frames are discarded.
    fn seek_to(&mut self, position: u64, playing: bool) {
        self.clock.set(position, playing);
        self.scheduler.seek(position, self.looping);

        #[cfg(feature = "audio")]
        if let Some(audio) = self.audio {
            audio.reset(position, self.config.fps, playing, self.looping);
        }
    }

    fn seek_to_frame(&mut self, frame: i64) {
        let frame = frame.clamp(0, self.last_frame() as i64) as u64;
        self.seek_to(frame, self.clock.is_playing());
    }

    fn toggle_play(&mut self) {
        let frame = self.current_frame();
        if self.clock.is_playing() {
            self.seek_to(frame as u64, false);
        } else if !self.looping && frame >= self.last_frame() {
            self.seek_to(0, true);
        } else {
            self.seek_to(frame as u64, true);
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
            self.current = Some(frame);
            self.frames_shown += 1;
            self.needs_redraw = true;
        }

        if self.needs_redraw {
            self.needs_redraw = false;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        self.update_title();
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
            title += &format!(" | {:.1}/{fps} fps", self.shown_fps);
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

        let frame = self.current_frame() as i64;
        let second = self.config.fps as i64;
        match event.logical_key.as_ref() {
            Key::Named(NamedKey::Space) => self.toggle_play(),
            Key::Named(NamedKey::Escape) | Key::Character("q") => event_loop.exit(),
            Key::Named(NamedKey::ArrowLeft) | Key::Character("h") => {
                self.seek_to_frame(frame - second)
            }
            Key::Named(NamedKey::ArrowRight) | Key::Character("l") => {
                self.seek_to_frame(frame + second)
            }
            Key::Named(NamedKey::ArrowDown) | Key::Character("j" | ",") => self.step(frame - 1),
            Key::Named(NamedKey::ArrowUp) | Key::Character("k" | ".") => self.step(frame + 1),
            Key::Named(NamedKey::Home) | Key::Character("g") => self.seek_to_frame(0),
            Key::Named(NamedKey::End) | Key::Character("G") => {
                self.seek_to_frame(self.last_frame() as i64)
            }
            Key::Character("r") => {
                self.looping = !self.looping;
                let playing = self.clock.is_playing();
                self.seek_to(frame as u64, playing);
            }
            Key::Character("b") => {
                self.show_overlay = !self.show_overlay;
                self.needs_redraw = true;
            }
            Key::Character(digit) if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
                let tenth = (digit.as_bytes()[0] - b'0') as i64;
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

    fn seek_to_cursor(&mut self) {
        let Some(window) = &self.window else { return };
        let width = window.inner_size().width.max(1) as f64;
        let progress = (self.cursor.x / width).clamp(0., 1.);
        let frame = (progress * self.last_frame() as f64).round() as i64;
        self.seek_to_frame(frame);
    }

    fn is_cursor_on_seek_bar(&self) -> bool {
        self.window.as_ref().is_some_and(|window| {
            let bar_top =
                window.inner_size().height as f64 - SEEK_BAR_HIT_HEIGHT * window.scale_factor();
            self.cursor.y >= bar_top
        })
    }

    fn redraw(&mut self) -> Result<(), PlayerError> {
        let overlay = Overlay {
            visible: self.show_overlay,
            progress: self.current_frame() as f32 / self.last_frame().max(1) as f32,
            paused: !self.clock.is_playing(),
        };

        if let Some(presenter) = self.presenter.as_mut() {
            presenter.present(self.current.as_ref().map(|(_, tree)| tree), &overlay)?;
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

        let attributes = Window::default_attributes()
            .with_title(self.config.title)
            .with_inner_size(self.config.window_size);

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => return self.fail(event_loop, err.into()),
        };

        match Presenter::new(window.clone(), self.config.backend, self.config.background) {
            Ok(presenter) => self.presenter = Some(presenter),
            Err(err) => return self.fail(event_loop, err),
        }

        self.window = Some(window);
        self.needs_redraw = true;
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: FrameReady) {
        self.tick();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(Err(err)) = self.presenter.as_mut().map(Presenter::resize) {
                    return self.fail(event_loop, err);
                }
                self.needs_redraw = true;
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.redraw() {
                    self.fail(event_loop, err);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, event),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if self.seeking_with_mouse {
                    self.seek_to_cursor();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed if self.is_cursor_on_seek_bar() => {
                    self.seeking_with_mouse = true;
                    self.seek_to_cursor();
                }
                ElementState::Pressed => self.toggle_play(),
                ElementState::Released => self.seeking_with_mouse = false,
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

fn format_time(frame: usize, fps: usize) -> String {
    let seconds = frame as f64 / fps.max(1) as f64;
    format!("{}:{:05.2}", (seconds / 60.) as u64, seconds % 60.)
}
