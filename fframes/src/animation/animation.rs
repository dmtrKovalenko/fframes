use super::{cubic_bezier::CubicBezierRuntime, spring};
use std::ops::Range;

#[derive(Clone, Debug)]
/// Resolved easing function.
pub enum AnimationRuntime {
    /// No animation, used internally for filling gaps keyframe
    Static(f32),
    SpringRuntime(spring::SpringRuntime, f32),
    Linear(f32),
    CubicBezier(CubicBezierRuntime, f32),
}

impl AnimationRuntime {
    pub fn new(tween_duration: f32, easing: &Easing) -> Self {
        match easing {
            Easing::Spring {
                mass,
                stiffness,
                damping,
            } => {
                let spring_runtime = spring::SpringRuntime::new(*mass, *stiffness, *damping);

                AnimationRuntime::SpringRuntime(
                    spring_runtime,
                    spring_runtime.get_duration().min(tween_duration),
                )
            }
            Easing::Linear => AnimationRuntime::Linear(tween_duration),
            Easing::EaseIn => {
                AnimationRuntime::CubicBezier(CubicBezierRuntime::ease_in(), tween_duration)
            }
            Easing::EaseOut => {
                AnimationRuntime::CubicBezier(CubicBezierRuntime::ease_out(), tween_duration)
            }
            Easing::EaseInOut => {
                AnimationRuntime::CubicBezier(CubicBezierRuntime::ease_in_out(), tween_duration)
            }

            Easing::CubicBezier(x1, y1, x2, y2) => AnimationRuntime::CubicBezier(
                CubicBezierRuntime::new(*x1, *y1, *x2, *y2),
                tween_duration,
            ),
        }
    }
}

impl AnimationRuntime {
    pub fn get_duration(&self) -> f32 {
        match *self {
            AnimationRuntime::Linear(duration) => duration,
            AnimationRuntime::SpringRuntime(_spring, duration) => duration,
            AnimationRuntime::CubicBezier(_, duration) => duration,
            AnimationRuntime::Static(duration) => duration,
        }
    }

    pub fn solve(&self, t: &f32) -> f32 {
        match &self {
            &AnimationRuntime::Linear(duration) => t / duration,
            &AnimationRuntime::SpringRuntime(spring, _) => spring.solve(t),
            &AnimationRuntime::CubicBezier(cubic_bezier, duration) => {
                cubic_bezier.solve(t / duration)
            }
            AnimationRuntime::Static(_) => 0.,
        }
    }
}

/// animation easing. Different variants of how value changes over time.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Easing {
    /// Specifies an animation with the same speed from start to end.
    /// calculates as Linear(duration): f(current_time) = current_time / duration
    Linear,
    /// CSS-like ease-in easing function.
    /// Specifies an animation with a slow start.
    EaseIn,
    /// CSS-like ease-out easing function.
    /// Specifies an animation with a slow start and end, and faster in the middle.
    EaseOut,
    /// CSS-like ease-in-out easing function.
    /// Specifies an animation with a slow start and end, and faster in the middle.
    EaseInOut,
    /// CSS-like cubic-bezier easing function.
    /// Defines as a cubic-bezier(x1, y1, x2, y2) where x1, y1, x2, y2 are numbers in the range [0, 1].
    CubicBezier(f32, f32, f32, f32),
    // Inspired by https://webkit.org/demos/spring/spring.js. Copyright (C) 2016 Apple Inc. All rights reserved.
    /// Specifies an animation that calculates value based on spring physics.
    /// Learn more about spring physics: <https://www.joshwcomeau.com/animation/a-friendly-introduction-to-spring-physics/>
    Spring {
        mass: f32,
        stiffness: f32,
        damping: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct KeyFrame<'a, T: Animatable> {
    /// Start time of the keyframe in seconds
    pub start: f32,
    /// End time of the keyframe in seconds, it is going to be used as an easing duration if specified leaving the time before the next keyframe as a static value.
    pub end: Option<f32>,
    pub to: T,
    pub from: T,
    pub easing: &'a Easing,
}

pub trait Animatable: Copy {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self;
}

impl Animatable for f32 {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        let animation_range = to - self;

        self + animation_range * progress
    }
}

impl Animatable for f64 {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        let animation_range = to - self;

        self + animation_range * progress as f64
    }
}

impl Animatable for (f32, f32) {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        (
            self.0.apply_progress(&to.0, progress),
            self.1.apply_progress(&to.1, progress),
        )
    }
}

impl Animatable for (f32, f32, f32) {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        (
            self.0.apply_progress(&to.0, progress),
            self.1.apply_progress(&to.1, progress),
            self.2.apply_progress(&to.2, progress),
        )
    }
}

impl Animatable for (f32, f32, f32, f32) {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        (
            self.0.apply_progress(&to.0, progress),
            self.1.apply_progress(&to.1, progress),
            self.2.apply_progress(&to.2, progress),
            self.3.apply_progress(&to.3, progress),
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Tween<T: Animatable + Copy> {
    pub(crate) seconds_range: Range<f32>,
    pub(crate) from: T,
    pub(crate) to: T,
    pub(crate) animation_runtime: AnimationRuntime,
}

#[derive(Debug)]
pub struct KeyFramesAnimation<T: Animatable + Copy + Default> {
    pub total_duration: f32,
    pub final_value: T,
    pub(crate) keyframes: Vec<Tween<T>>,
}

impl<T: Animatable + Copy + Default> KeyFramesAnimation<T> {
    pub fn new(mut tweens: Vec<KeyFrame<T>>) -> Self {
        if tweens.is_empty() {
            crate::log!(
                "WARN: some of the keyframe animations did not receive any keyframes. Behaviour is undefined."
            );

            return KeyFramesAnimation {
                keyframes: vec![],
                total_duration: 0.,
                final_value: T::default(),
            };
        }

        tweens.sort_unstable_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut keyframes = tweens
            .iter()
            .enumerate()
            .flat_map(|(i, tween)| {
                let next_start = tweens.get(i + 1).map(|tween| tween.start);
                let tween_duration = match (tween.end, next_start) {
                    (Some(end), _) => end - tween.start,
                    (None, Some(next_start)) => next_start - tween.start,
                    // easing is a specific use case, we can infer the duration of a tween based on
                    // the parameters of the easing function. Defined duration just truncates the
                    // animation
                    (None, None) if matches!(tween.easing, Easing::Spring { .. }) => f32::MAX,
                    (None, None) => {
                        crate::log!("WARN: for a last kefyrame starting at {:?} there is no duration defined. Skipping", tween.start);

                        return vec![];
                    },
                };

                let animation_runtime = AnimationRuntime::new(tween_duration, tween.easing);
                let keyframe = Tween {
                    to: tween.to,
                    from: tween.from,
                    seconds_range: (tween.start..tween.start + animation_runtime.get_duration()),
                    animation_runtime,
                };

                match tweens.get(i + 1) {
                    None => vec![keyframe],
                    Some(next_tween) if next_tween.start <= keyframe.seconds_range.end => {
                        vec![keyframe]
                    }
                    Some(next_tween) => {
                        let filler_keyframe_range = keyframe.seconds_range.end..next_tween.start;
                        let filler_keyframe = Tween {
                            from: keyframe.to,
                            to: keyframe.to,
                            animation_runtime: AnimationRuntime::Static(
                                filler_keyframe_range.end - filler_keyframe_range.start,
                            ),
                            seconds_range: filler_keyframe_range,
                        };

                        vec![keyframe, filler_keyframe]
                    }
                }
            })
            .collect::<Vec<_>>();

        // safe to use [index access] because we invariant empty vectors at start
        if tweens[0].start > 0. {
            let seconds_range = 0f32..tweens[0].start;
            keyframes.insert(
                0,
                Tween {
                    from: tweens[0].from,
                    to: tweens[0].from,
                    animation_runtime: AnimationRuntime::Static(
                        seconds_range.end - seconds_range.start,
                    ),
                    seconds_range,
                },
            );
        }

        // this duplicates the logic. The last keyframe end is required, if not provided the
        // keyframe is ignored and we end at the start of the last keyframe.
        let last_tween = tweens.last().unwrap();
        let total_duration = last_tween.end.unwrap_or(last_tween.start) - tweens[0].start;

        KeyFramesAnimation {
            keyframes,
            total_duration,
            final_value: last_tween.to,
        }
    }
}

#[macro_export]
macro_rules! timeline {
    ($(at $start:expr $(=> $end:expr)?, animate $from:expr => $to:expr, $easing:expr),+ $(,)?) => {
        fframes::animation::KeyFramesAnimation::new(vec![
            $(
                fframes::animation::KeyFrame {
                    start: $start,
                    end: $crate::option_literal!($($end)?),
                    from: $from,
                    to: $to,
                    easing: &$easing,
                },
            )+
        ])
    };

    ($(at $start:expr $(, duration $duration:expr)?, animate $from:expr => $to:expr, $easing:expr),+ $(,)?) => {
        fframes::animation::KeyFramesAnimation::new(vec![
            $(
                fframes::animation::KeyFrame {
                    start: $start,
                    end: $crate::option_duration!($start, $($duration)?),
                    from: $from,
                    to: $to,
                    easing: &$easing,
                },
            )+
        ])
    };
}

#[macro_export]
macro_rules! option_duration {
    ($start: expr) => {
        None
    };
    ($start: expr, $end:expr) => {
        Some($start + $end)
    };
}

#[macro_export]
macro_rules! option_literal {
    () => {
        None
    };
    ($e:expr) => {
        Some($e)
    };
}
