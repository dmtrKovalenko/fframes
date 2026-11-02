//! The motion of the player's chrome: nothing pops. Hover fills and the toggle state fade,
//! a pressed button gives under the pointer, the play glyph turns into the pause glyph, a
//! hovered scene lights up, thumbnails resolve as they arrive, and a play or pause toggled
//! on the video itself (a click, the space key) flashes its glyph in the middle of the
//! picture, where the eye is. Every value is a [`Tween`] retargeted from wherever it is,
//! so an interrupted motion never jumps.

use std::time::{Duration, Instant};

use crate::controls::Control;

/// A CSS style cubic bezier easing, `(x1, y1, x2, y2)`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Easing(f32, f32, f32, f32);

impl Easing {
    /// Travel between two states: a departure, clear travel and a soft arrival.
    pub(crate) const TRAVEL: Easing = Easing(0.45, 0., 0.1, 1.);
    /// Arrival from nowhere: fast out of the gate, a long settle.
    pub(crate) const ARRIVAL: Easing = Easing(0.25, 0.8, 0.25, 1.);
    /// UI feedback: as quick as the system.
    pub(crate) const EASE_OUT: Easing = Easing(0., 0., 0.58, 1.);

    fn bezier(p1: f32, p2: f32, t: f32) -> f32 {
        let u = 1. - t;
        3. * u * u * t * p1 + 3. * u * t * t * p2 + t * t * t
    }

    pub(crate) fn apply(self, x: f32) -> f32 {
        if !(x > 0. && x < 1.) {
            return x.clamp(0., 1.);
        }
        // Solve x(t) = x by bisection, the curve is monotonic in x.
        let (mut low, mut high) = (0_f32, 1_f32);
        let mut t = x;
        for _ in 0..24 {
            let cx = Self::bezier(self.0, self.2, t);
            if (cx - x).abs() < 1e-4 {
                break;
            }
            if cx < x {
                low = t;
            } else {
                high = t;
            }
            t = f32::midpoint(low, high);
        }
        Self::bezier(self.1, self.3, t)
    }
}

/// A value moving from where it is to a target over a duration.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    easing: Easing,
}

impl Tween {
    pub(crate) fn at(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            start: now,
            duration: Duration::ZERO,
            easing: Easing::EASE_OUT,
        }
    }

    pub(crate) fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.start).as_secs_f32();
        let progress = self.easing.apply(elapsed / self.duration.as_secs_f32());
        self.from + (self.to - self.from) * progress
    }

    pub(crate) fn is_moving(&self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.start) < self.duration
    }

    /// Heads for `to`, starting from the current value so an interrupted motion continues
    /// instead of jumping. A target that is already set is left alone.
    pub(crate) fn go(&mut self, to: f32, duration: Duration, easing: Easing, now: Instant) {
        if self.to == to {
            return;
        }
        self.from = self.value(now);
        self.to = to;
        self.start = now;
        self.duration = duration;
        self.easing = easing;
    }
}

/// A glyph flashed in the middle of the frame when playback is toggled on the video.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Flash {
    pub started: Instant,
    pub playing: bool,
}

impl Flash {
    pub(crate) const DURATION: Duration = Duration::from_millis(560);

    /// `0.0..1.0` through its life, `None` once it is over.
    pub(crate) fn progress(&self, now: Instant) -> Option<f32> {
        let elapsed = now.saturating_duration_since(self.started);
        (elapsed < Self::DURATION).then(|| elapsed.as_secs_f32() / Self::DURATION.as_secs_f32())
    }
}

/// What the player wants its chrome to show; the motion follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Intent {
    pub playing: bool,
    pub looping: bool,
    pub hovered: Option<Control>,
    pub pressed: Option<Control>,
}

/// How long each motion takes. `INSTANT` makes every one a cut, which is how the player
/// behaved before it moved (used for review films).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub hover: Duration,
    pub press: Duration,
    pub release: Duration,
    pub glyph: Duration,
    /// A thumbnail that just arrived fades in over this long.
    pub thumbnail: Duration,
    pub flash: bool,
}

impl Timing {
    pub(crate) const DEFAULT: Timing = Timing {
        hover: Duration::from_millis(140),
        press: Duration::from_millis(80),
        release: Duration::from_millis(240),
        glyph: Duration::from_millis(240),
        thumbnail: Duration::from_millis(260),
        flash: true,
    };

    #[cfg(test)]
    pub(crate) const INSTANT: Timing = Timing {
        hover: Duration::ZERO,
        press: Duration::ZERO,
        release: Duration::ZERO,
        glyph: Duration::ZERO,
        thumbnail: Duration::ZERO,
        flash: false,
    };
}

/// The buttons that have a hover and a press of their own.
const BUTTONS: [Control; 6] = [
    Control::Back,
    Control::PlayPause,
    Control::Forward,
    Control::Seek,
    Control::Loop,
    Control::Fullscreen,
];

fn slot(control: Control) -> Option<usize> {
    BUTTONS.iter().position(|&c| c == control)
}

pub(crate) struct Motion {
    timing: Timing,
    /// One per button: 0 at rest, 1 hovered.
    hover: [Tween; 6],
    /// One per button: 0 at rest, 1 held down.
    press: [Tween; 6],
    /// The primary glyph: 0 play, 1 pause.
    glyph: Tween,
    /// The loop toggle: 0 off, 1 on.
    looping: Tween,
    /// The scene under the pointer (by its first frame) and how lit it is.
    scene: Option<usize>,
    scene_hover: Tween,
    flash: Option<Flash>,
}

impl Motion {
    pub(crate) fn new(intent: Intent, timing: Timing, now: Instant) -> Self {
        let on = |yes: bool| Tween::at(f32::from(u8::from(yes)), now);
        let mut motion = Self {
            timing,
            hover: [Tween::at(0., now); 6],
            press: [Tween::at(0., now); 6],
            glyph: on(intent.playing),
            looping: on(intent.looping),
            scene: None,
            scene_hover: Tween::at(0., now),
            flash: None,
        };
        motion.follow(intent, now);
        motion
    }

    /// Retargets every value towards `intent`.
    pub(crate) fn follow(&mut self, intent: Intent, now: Instant) {
        let t = self.timing;
        let on = |yes: bool| f32::from(u8::from(yes));

        self.glyph
            .go(on(intent.playing), t.glyph, Easing::TRAVEL, now);
        self.looping
            .go(on(intent.looping), t.hover, Easing::EASE_OUT, now);

        for (index, control) in BUTTONS.iter().enumerate() {
            let hovered = intent.hovered == Some(*control);
            self.hover[index].go(on(hovered), t.hover, Easing::EASE_OUT, now);
            let pressed = intent.pressed == Some(*control);
            if pressed {
                self.press[index].go(1., t.press, Easing::EASE_OUT, now);
            } else {
                self.press[index].go(0., t.release, Easing::ARRIVAL, now);
            }
        }

        // A scene lights up under the pointer; moving straight onto another scene moves
        // the light, leaving fades it where it was
        if let Some(Control::Scene(start)) = intent.hovered {
            if self.scene != Some(start) {
                self.scene = Some(start);
                if self.scene_hover.value(now) > 0. {
                    self.scene_hover = Tween::at(1., now);
                }
            }
            self.scene_hover.go(1., t.hover, Easing::EASE_OUT, now);
        } else {
            self.scene_hover.go(0., t.hover, Easing::EASE_OUT, now);
        }
    }

    /// Playback was toggled somewhere other than a play button.
    pub(crate) fn flash(&mut self, playing: bool, now: Instant) {
        if self.timing.flash {
            self.flash = Some(Flash {
                started: now,
                playing,
            });
        }
    }

    /// Whether another frame is needed to finish a motion.
    pub(crate) fn is_moving(&self, now: Instant) -> bool {
        self.glyph.is_moving(now)
            || self.looping.is_moving(now)
            || self.scene_hover.is_moving(now)
            || self.hover.iter().any(|t| t.is_moving(now))
            || self.press.iter().any(|t| t.is_moving(now))
            || self.flash.is_some_and(|f| f.progress(now).is_some())
    }

    /// The values to draw with at `now`.
    pub(crate) fn sample(&self, now: Instant) -> Sample {
        let mut hover = [0.; 6];
        let mut press = [0.; 6];
        for index in 0..6 {
            hover[index] = self.hover[index].value(now);
            press[index] = self.press[index].value(now);
        }
        Sample {
            now,
            thumbnail_fade: self.timing.thumbnail,
            hover,
            press,
            glyph: self.glyph.value(now),
            looping: self.looping.value(now),
            scene: self
                .scene
                .map(|start| (start, self.scene_hover.value(now)))
                .filter(|(_, lit)| *lit > 0.),
            flash: self
                .flash
                .and_then(|f| f.progress(now).map(|p| (p, f.playing))),
        }
    }
}

/// A snapshot of every animated value, all in `0.0..=1.0`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sample {
    /// When the sample was taken.
    pub now: Instant,
    /// How long an arriving thumbnail takes to fade in.
    pub thumbnail_fade: Duration,
    hover: [f32; 6],
    press: [f32; 6],
    pub glyph: f32,
    pub looping: f32,
    /// The hovered scene's first frame and how lit it is.
    pub scene: Option<(usize, f32)>,
    /// Progress and the state it announces, while a flash is up.
    pub flash: Option<(f32, bool)>,
}

impl Sample {
    /// Every motion at rest in `intent`'s state.
    #[cfg(test)]
    pub(crate) fn settled(intent: Intent) -> Self {
        let on = |yes: bool| f32::from(u8::from(yes));
        let mut sample = Self {
            now: Instant::now(),
            thumbnail_fade: Duration::ZERO,
            hover: [0.; 6],
            press: [0.; 6],
            glyph: on(intent.playing),
            looping: on(intent.looping),
            scene: match intent.hovered {
                Some(Control::Scene(start)) => Some((start, 1.)),
                _ => None,
            },
            flash: None,
        };
        if let Some(index) = intent.hovered.and_then(slot) {
            sample.hover[index] = 1.;
        }
        if let Some(index) = intent.pressed.and_then(slot) {
            sample.press[index] = 1.;
        }
        sample
    }

    pub(crate) fn hover(&self, control: Control) -> f32 {
        slot(control).map_or(0., |index| self.hover[index])
    }

    pub(crate) fn press(&self, control: Control) -> f32 {
        slot(control).map_or(0., |index| self.press[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent() -> Intent {
        Intent {
            playing: true,
            looping: false,
            hovered: None,
            pressed: None,
        }
    }

    #[test]
    fn easing_ends_are_exact_and_the_curves_rise() {
        for easing in [Easing::TRAVEL, Easing::ARRIVAL, Easing::EASE_OUT] {
            assert_eq!(easing.apply(0.), 0.);
            assert_eq!(easing.apply(1.), 1.);
            assert_eq!(easing.apply(-1.), 0.);
            assert_eq!(easing.apply(2.), 1.);
            let steps: Vec<f32> = (0..=10).map(|i| easing.apply(i as f32 / 10.)).collect();
            assert!(steps.windows(2).all(|w| w[1] >= w[0]), "{steps:?}");
        }
        assert!(Easing::ARRIVAL.apply(0.3) > 0.5);
        assert!(Easing::TRAVEL.apply(0.5) > 0.5);
    }

    #[test]
    fn a_retargeted_tween_continues_from_where_it_is() {
        let start = Instant::now();
        let mut tween = Tween::at(0., start);
        tween.go(1., Duration::from_millis(100), Easing::EASE_OUT, start);
        let midway = start + Duration::from_millis(50);
        let value = tween.value(midway);
        assert!(value > 0.5 && value < 1., "{value}");
        assert!(tween.is_moving(midway));

        tween.go(0., Duration::from_millis(100), Easing::EASE_OUT, midway);
        assert_eq!(tween.value(midway), value);
        assert_eq!(tween.value(midway + Duration::from_millis(100)), 0.);
        assert!(!tween.is_moving(midway + Duration::from_millis(100)));
    }

    #[test]
    fn the_motion_follows_the_intent_and_settles() {
        let now = Instant::now();
        let mut motion = Motion::new(intent(), Timing::DEFAULT, now);
        assert!(!motion.is_moving(now));
        assert_eq!(motion.sample(now).glyph, 1.);

        motion.follow(
            Intent {
                playing: false,
                hovered: Some(Control::Loop),
                ..intent()
            },
            now,
        );
        let later = now + Duration::from_millis(10);
        assert!(motion.is_moving(later));
        let sample = motion.sample(later);
        assert!(sample.glyph < 1. && sample.glyph > 0.);
        assert!(sample.hover(Control::Loop) > 0.);
        assert_eq!(sample.hover(Control::Back), 0.);

        let settled = now + Duration::from_secs(1);
        assert!(!motion.is_moving(settled));
        let sample = motion.sample(settled);
        assert_eq!(sample.glyph, 0.);
        assert_eq!(sample.hover(Control::Loop), 1.);

        motion.flash(true, settled);
        assert!(motion.is_moving(settled));
        assert!(!motion.is_moving(settled + Flash::DURATION));
    }

    #[test]
    fn a_hovered_scene_lights_up_and_the_light_moves_between_scenes() {
        let now = Instant::now();
        let mut motion = Motion::new(intent(), Timing::DEFAULT, now);
        motion.follow(
            Intent {
                hovered: Some(Control::Scene(0)),
                ..intent()
            },
            now,
        );
        let later = now + Duration::from_secs(1);
        assert_eq!(motion.sample(later).scene, Some((0, 1.)));

        motion.follow(
            Intent {
                hovered: Some(Control::Scene(60)),
                ..intent()
            },
            later,
        );
        assert_eq!(motion.sample(later).scene, Some((60, 1.)));

        motion.follow(intent(), later);
        let fading = motion.sample(later + Duration::from_millis(50)).scene;
        assert!(
            matches!(fading, Some((60, lit)) if lit > 0. && lit < 1.),
            "{fading:?}"
        );
        assert_eq!(motion.sample(later + Duration::from_secs(1)).scene, None);
    }

    #[test]
    fn instant_timing_cuts() {
        let now = Instant::now();
        let mut motion = Motion::new(intent(), Timing::INSTANT, now);
        motion.follow(
            Intent {
                playing: false,
                pressed: Some(Control::PlayPause),
                ..intent()
            },
            now,
        );
        motion.flash(false, now);
        assert!(!motion.is_moving(now));
        let sample = motion.sample(now);
        assert_eq!(sample.glyph, 0.);
        assert_eq!(sample.press(Control::PlayPause), 1.);
        assert!(sample.flash.is_none());
    }
}
