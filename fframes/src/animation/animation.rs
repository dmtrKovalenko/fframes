use super::spring;
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
/// Resolved easing function.
pub enum AnimationRuntime {
    /// No animation, used internally for filling gaps keyframe
    Static(f32),
    Linear(f32),
    SpringRuntime(spring::SpringRuntime, f32),
}

impl From<&Easing> for AnimationRuntime {
    fn from(easing: &Easing) -> Self {
        match easing {
            Easing::Spring(options) => {
                let spring_runtime = spring::SpringRuntime::from_options(options);
                let duration = spring_runtime.get_duration();

                AnimationRuntime::SpringRuntime(spring_runtime, duration)
            }
            Easing::Linear(duration) => AnimationRuntime::Linear(*duration),
            Easing::Spring2(mass, stiffness, damping) => {
                let spring_runtime =
                    spring::SpringRuntime::from_options(&crate::animation::SpringOptions {
                        mass: *mass,
                        stiffness: *stiffness,
                        damping: *damping,
                    });

                let duration = spring_runtime.get_duration();
                AnimationRuntime::SpringRuntime(spring_runtime, duration)
            }
        }
    }
}

impl AnimationRuntime {
    pub fn get_duration(&self) -> f32 {
        match &self {
            AnimationRuntime::Linear(duration) => *duration,
            AnimationRuntime::SpringRuntime(_spring, duration) => *duration,
            AnimationRuntime::Static(_) => unreachable!(),
        }
    }

    pub fn solve(&self, t: &f32) -> f32 {
        match &self {
            &AnimationRuntime::Linear(duration) => t / duration,
            &AnimationRuntime::SpringRuntime(spring, _) => spring.solve(t),
            AnimationRuntime::Static(_) => 1.,
        }
    }
}

/// animation easing. Different variants of how value changes over time.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Easing {
    /// Specifies an animation with the same speed from start to end.
    /// calculates as Linear(duration): f(current_time) = current_time / duration
    Linear(f32),
    // Inspired by https://webkit.org/demos/spring/spring.js. Copyright (C) 2016 Apple Inc. All rights reserved.
    Spring(spring::SpringOptions),
    /// Specifies an animation that calculates value based on spring physics.
    /// Learn more about spring physics: https://www.joshwcomeau.com/animation/a-friendly-introduction-to-spring-physics/
    ///
    /// Mass, Stiffness, Damping
    Spring2(f32, f32, f32),
}

#[derive(Clone, Copy, Debug)]
pub struct KeyFrame<'a, T: Animatable> {
    pub start: f32,
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

#[derive(Clone, Debug)]
pub(crate) struct Tween<T: Animatable + Copy> {
    pub(crate) seconds_range: Range<f32>,
    pub(crate) from: T,
    pub(crate) to: T,
    pub(crate) animation_runtime: AnimationRuntime,
}

#[derive(Debug)]
pub struct KeyFramesAnimation<T: Animatable + Copy> {
    pub(crate) keyframes: Vec<Tween<T>>,
}

impl<T: Animatable + Copy> KeyFramesAnimation<T> {
    pub fn new(tweens: Vec<KeyFrame<T>>) -> Self {
        let mut sorted_tweens = tweens;
        sorted_tweens.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut keyframes = sorted_tweens
            .iter()
            .enumerate()
            .flat_map(|(i, tween)| {
                let animation_runtime = AnimationRuntime::from(tween.easing);
                let keyframe = Tween {
                    to: tween.to,
                    from: tween.from,
                    seconds_range: (tween.start..tween.start + animation_runtime.get_duration()),
                    animation_runtime,
                };

                match sorted_tweens.get(i + 1) {
                    None => vec![keyframe],
                    Some(next_tween) if next_tween.start <= keyframe.seconds_range.end => {
                        vec![keyframe]
                    }
                    Some(next_tween) => {
                        let filler_keyframe_range = keyframe.seconds_range.end..next_tween.start;
                        let filler_keyframe = Tween {
                            from: keyframe.to,
                            to: keyframe.to,
                            animation_runtime: AnimationRuntime::Linear(
                                filler_keyframe_range.end - filler_keyframe_range.start,
                            ),
                            seconds_range: filler_keyframe_range,
                        };

                        vec![keyframe, filler_keyframe]
                    }
                }
            })
            .collect::<Vec<_>>();

        if sorted_tweens[0].start > 0. {
            let seconds_range = 0f32..sorted_tweens[0].start;
            keyframes.insert(
                0,
                Tween {
                    from: sorted_tweens[0].from,
                    to: sorted_tweens[0].from,
                    animation_runtime: AnimationRuntime::Static(
                        seconds_range.end - seconds_range.start,
                    ),
                    seconds_range,
                },
            );
        }

        let last_keyframe = &keyframes[keyframes.len() - 1];
        if last_keyframe.seconds_range.end < f32::MAX {
            let last_filling_keyframe = Tween {
                from: last_keyframe.to,
                to: last_keyframe.to,
                animation_runtime: AnimationRuntime::Static(
                    f32::MAX - last_keyframe.seconds_range.end,
                ),
                seconds_range: last_keyframe.seconds_range.end..f32::MAX,
            };

            keyframes.push(last_filling_keyframe);
        }

        KeyFramesAnimation { keyframes }
    }
}

#[macro_export]
macro_rules! timeline {
    ($(on $start: expr, val $from:expr => $to:expr, $easing:expr),+) => {
        fframes::animation::KeyFramesAnimation::new(vec![
           $(
            fframes::animation::KeyFrame {
                start: $start,
                from: $from,
                to: $to,
                easing: &$easing,
            }
           ),+
    ])
    };
}
