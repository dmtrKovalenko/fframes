//! Addressing frames and frame ranges by frame number, time, percentage or scene.
//!
//! | spec              | meaning                                                      |
//! |-------------------|--------------------------------------------------------------|
//! | `120`, `120f`     | frame 120                                                    |
//! | `3.2s`, `1:05.5`  | a timestamp (`m:ss`, `h:mm:ss` work too)                     |
//! | `50%`             | a fraction of the whole video                                |
//! | `start`, `end`    | the first / last frame of the video                          |
//! | `Intro`           | the first frame of the scene named `Intro` (case-insensitive)|
//! | `#3`              | the first frame of the scene with index 3 (0-based)          |
//! | `Intro[1]`        | the second scene of type `Intro`                             |
//! | `Intro@1.2s`      | 1.2 seconds into the scene (also `@12`, `@50%`, `@end`)      |
//!
//! Ranges are `a..b` (end exclusive), `a..`, `..b`, or a single scene (`Intro`) for the whole
//! scene. A scene name on the right side of `..` means the end of that scene.
use crate::{ResolvedScenesTimeline, SceneInfo};
use std::fmt;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum TimeSpecError {
    Invalid(String),
    UnknownScene {
        query: String,
        available: Vec<String>,
    },
    AmbiguousScene {
        query: String,
        indexes: Vec<usize>,
    },
    OutOfRange {
        frame: usize,
        duration: usize,
    },
    EmptyRange(Range<usize>),
}

impl fmt::Display for TimeSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(spec) => write!(
                f,
                "can not parse \"{spec}\": use a frame (120), time (3.2s, 1:05), percentage (50%), \
                 scene (Intro, #3, Intro@1.5s) or start/end"
            ),
            Self::UnknownScene { query, available } => {
                write!(f, "no scene \"{query}\"")?;
                if available.is_empty() {
                    write!(f, ", the video does not define scenes")
                } else {
                    write!(f, ", available: {}", available.join(", "))
                }
            }
            Self::AmbiguousScene { query, indexes } => write!(
                f,
                "\"{query}\" matches scenes {indexes:?}, use {query}[n] or #index"
            ),
            Self::OutOfRange { frame, duration } => {
                write!(f, "frame {frame} is outside the video (0..{duration})")
            }
            Self::EmptyRange(range) => write!(f, "range {range:?} is empty"),
        }
    }
}

impl std::error::Error for TimeSpecError {}

/// A scene as seen by the addressing: its name and resolved frames (overlaps included).
#[derive(Debug, Clone)]
pub struct TimelineScene {
    pub index: usize,
    /// Short name, e.g. `Intro` for `my_video::scenes::Intro`.
    pub name: String,
    /// Name as returned by `Scene::name`.
    pub full_name: String,
    pub frames: Range<usize>,
}

/// Everything needed to resolve time specs of one video.
#[derive(Debug, Clone)]
pub struct TimelineIndex {
    pub fps: usize,
    pub duration_in_frames: usize,
    pub scenes: Vec<TimelineScene>,
}

/// `my_video::scenes::Intro<'_>` -> `Intro`
pub fn short_scene_name(name: &str) -> &str {
    let without_generics = name.split('<').next().unwrap_or(name);
    without_generics
        .rsplit("::")
        .next()
        .unwrap_or(without_generics)
}

impl TimelineIndex {
    pub fn new(
        fps: usize,
        duration_in_frames: usize,
        scenes: Option<&ResolvedScenesTimeline<'_>>,
    ) -> Self {
        Self {
            fps,
            duration_in_frames,
            scenes: scenes
                .map(|scenes| {
                    scenes
                        .timeline
                        .iter()
                        .map(|(range, info, scene)| TimelineScene {
                            index: info.index,
                            name: short_scene_name(scene.name()).to_owned(),
                            full_name: scene.name().to_owned(),
                            frames: range.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn duration_in_seconds(&self) -> f32 {
        self.duration_in_frames as f32 / self.fps as f32
    }

    pub fn frame_to_seconds(&self, frame: usize) -> f32 {
        frame as f32 / self.fps as f32
    }

    /// Scenes that are visible at a frame (two while scenes overlap).
    pub fn scenes_at(&self, frame: usize) -> impl Iterator<Item = &TimelineScene> {
        self.scenes
            .iter()
            .filter(move |s| s.frames.contains(&frame))
    }

    pub fn full_range(&self) -> Range<usize> {
        0..self.duration_in_frames
    }

    fn find_scene(&self, query: &str) -> Result<&TimelineScene, TimeSpecError> {
        if let Some(index) = query.strip_prefix('#') {
            let index: usize = index
                .parse()
                .map_err(|_| TimeSpecError::Invalid(query.to_owned()))?;
            return self
                .scenes
                .get(index)
                .ok_or_else(|| self.unknown_scene(query));
        }

        let (name, nth) = match query.strip_suffix(']').and_then(|q| q.split_once('[')) {
            Some((name, nth)) => (
                name,
                Some(
                    nth.parse::<usize>()
                        .map_err(|_| TimeSpecError::Invalid(query.to_owned()))?,
                ),
            ),
            None => (query, None),
        };

        // `intro`, `Intro`, `intro_scene` and `IntroScene` all name the scene `IntroScene`.
        let normalize = |s: &str| {
            let s = s.replace(['_', '-', ' '], "").to_lowercase();
            match s.strip_suffix("scene") {
                Some(name) if !name.is_empty() => name.to_owned(),
                _ => s,
            }
        };
        let wanted = normalize(name);
        let matches: Vec<&TimelineScene> = self
            .scenes
            .iter()
            .filter(|s| normalize(&s.name) == wanted || normalize(&s.full_name) == wanted)
            .collect();

        match (matches.as_slice(), nth) {
            ([], _) => Err(self.unknown_scene(query)),
            ([scene], None) => Ok(scene),
            (all, None) => Err(TimeSpecError::AmbiguousScene {
                query: name.to_owned(),
                indexes: all.iter().map(|s| s.index).collect(),
            }),
            (all, Some(nth)) => all
                .get(nth)
                .copied()
                .ok_or_else(|| self.unknown_scene(query)),
        }
    }

    fn unknown_scene(&self, query: &str) -> TimeSpecError {
        TimeSpecError::UnknownScene {
            query: query.to_owned(),
            available: self
                .scenes
                .iter()
                .map(|s| format!("#{} {}", s.index, s.name))
                .collect(),
        }
    }

    /// Resolves a single point (`3.2s`, `Intro@50%`, ...) to a frame index inside the video.
    pub fn resolve_frame(&self, spec: &str) -> Result<usize, TimeSpecError> {
        let frame = self.resolve_point(spec.trim(), Side::Start)?;
        if frame >= self.duration_in_frames {
            return Err(TimeSpecError::OutOfRange {
                frame,
                duration: self.duration_in_frames,
            });
        }
        Ok(frame)
    }

    /// Resolves a range (`10s..20s`, `Intro`, `Intro..Outro`, `5s..`) to frames, end exclusive,
    /// clamped to the video.
    pub fn resolve_range(&self, spec: &str) -> Result<Range<usize>, TimeSpecError> {
        let spec = spec.trim();
        let range = if let Some((start, end)) = spec.split_once("..") {
            let start = if start.trim().is_empty() {
                0
            } else {
                self.resolve_point(start.trim(), Side::Start)?
            };
            let end = if end.trim().is_empty() {
                self.duration_in_frames
            } else {
                self.resolve_point(end.trim(), Side::End)?
            };
            start..end
        } else if matches!(spec, "all" | "*") {
            self.full_range()
        } else if let Ok(scene) = self.find_scene(spec) {
            scene.frames.clone()
        } else {
            // A single point is a one-frame range.
            let frame = self.resolve_point(spec, Side::Start)?;
            frame..frame + 1
        };

        let range =
            range.start.min(self.duration_in_frames)..range.end.min(self.duration_in_frames);
        if range.is_empty() {
            return Err(TimeSpecError::EmptyRange(range));
        }
        Ok(range)
    }

    fn resolve_point(&self, spec: &str, side: Side) -> Result<usize, TimeSpecError> {
        match spec {
            "start" => return Ok(0),
            "end" => {
                return Ok(match side {
                    Side::Start => self.duration_in_frames.saturating_sub(1),
                    Side::End => self.duration_in_frames,
                });
            }
            _ => {}
        }

        if let Some((scene, offset)) = spec.split_once('@') {
            let scene = self.find_scene(scene.trim())?;
            let length = scene.frames.len();
            let offset = match offset.trim() {
                "start" => 0,
                "end" => match side {
                    Side::Start => length.saturating_sub(1),
                    Side::End => length,
                },
                offset => self.parse_offset(offset, length)?,
            };
            return Ok(scene.frames.start + offset);
        }

        if let Some(offset) = self.try_parse_offset(spec, self.duration_in_frames) {
            return Ok(offset);
        }

        let scene = self.find_scene(spec).map_err(|err| match err {
            // Not a number and not a scene, the most useful message is the syntax.
            TimeSpecError::UnknownScene { available, .. } if available.is_empty() => {
                TimeSpecError::Invalid(spec.to_owned())
            }
            err => err,
        })?;

        Ok(match side {
            Side::Start => scene.frames.start,
            Side::End => scene.frames.end,
        })
    }

    fn parse_offset(&self, spec: &str, length: usize) -> Result<usize, TimeSpecError> {
        self.try_parse_offset(spec, length)
            .ok_or_else(|| TimeSpecError::Invalid(spec.to_owned()))
    }

    /// Frames, seconds, clock time or a percentage of `length` frames.
    fn try_parse_offset(&self, spec: &str, length: usize) -> Option<usize> {
        let to_frames = |seconds: f64| (seconds * self.fps as f64).round().max(0.) as usize;

        if let Some(percent) = spec.strip_suffix('%') {
            let percent: f64 = percent.trim().parse().ok()?;
            let frame = (length as f64 * percent / 100.).floor() as usize;
            // 100% is the last frame, not one past it.
            return Some(frame.min(length.saturating_sub(1)));
        }

        if let Some(seconds) = spec.strip_suffix("ms") {
            return Some(to_frames(seconds.trim().parse::<f64>().ok()? / 1000.));
        }

        if let Some(seconds) = spec.strip_suffix('s') {
            return Some(to_frames(seconds.trim().parse().ok()?));
        }

        if spec.contains(':') {
            let mut seconds = 0.;
            for part in spec.split(':') {
                seconds = seconds * 60. + part.trim().parse::<f64>().ok()?;
            }
            return Some(to_frames(seconds));
        }

        spec.strip_suffix('f').unwrap_or(spec).parse().ok()
    }
}

#[derive(Clone, Copy)]
enum Side {
    Start,
    End,
}

impl<'a> crate::FFramesContext<'a, '_> {
    /// Scenes active at a global frame together with their `Scene::name`. More than one scene
    /// is active while scenes overlap.
    pub fn scenes_at(
        &self,
        global_frame: usize,
    ) -> impl Iterator<Item = (&'a SceneInfo, &'static str)> + 'a {
        self.scenes
            .into_iter()
            .flat_map(|scenes| scenes.timeline.iter())
            .filter(move |(range, _, _)| range.contains(&global_frame))
            .map(|(_, info, scene)| (info, scene.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> TimelineIndex {
        TimelineIndex {
            fps: 30,
            duration_in_frames: 300,
            scenes: vec![
                TimelineScene {
                    index: 0,
                    name: "Intro".into(),
                    full_name: "video::Intro".into(),
                    frames: 0..100,
                },
                TimelineScene {
                    index: 1,
                    name: "Speaker".into(),
                    full_name: "video::Speaker".into(),
                    frames: 90..200,
                },
                TimelineScene {
                    index: 2,
                    name: "Speaker".into(),
                    full_name: "video::Speaker".into(),
                    frames: 200..300,
                },
            ],
        }
    }

    #[test]
    fn points() {
        let t = index();
        assert_eq!(t.resolve_frame("120"), Ok(120));
        assert_eq!(t.resolve_frame("120f"), Ok(120));
        assert_eq!(t.resolve_frame("2s"), Ok(60));
        assert_eq!(t.resolve_frame("500ms"), Ok(15));
        assert_eq!(t.resolve_frame("0:02.5"), Ok(75));
        assert_eq!(t.resolve_frame("50%"), Ok(150));
        assert_eq!(t.resolve_frame("100%"), Ok(299));
        assert_eq!(t.resolve_frame("end"), Ok(299));
        assert_eq!(t.resolve_frame("intro"), Ok(0));
        assert_eq!(t.resolve_frame("IntroScene"), Ok(0));
        assert_eq!(t.resolve_frame("intro_scene@1s"), Ok(30));
        assert_eq!(t.resolve_frame("#1"), Ok(90));
        assert_eq!(t.resolve_frame("Speaker[1]"), Ok(200));
        assert_eq!(t.resolve_frame("Intro@1s"), Ok(30));
        assert_eq!(t.resolve_frame("Intro@50%"), Ok(50));
        assert_eq!(t.resolve_frame("Intro@end"), Ok(99));
        assert!(matches!(
            t.resolve_frame("Speaker"),
            Err(TimeSpecError::AmbiguousScene { .. })
        ));
        assert!(matches!(
            t.resolve_frame("Outro"),
            Err(TimeSpecError::UnknownScene { .. })
        ));
        assert!(matches!(
            t.resolve_frame("11s"),
            Err(TimeSpecError::OutOfRange { .. })
        ));
    }

    #[test]
    fn ranges() {
        let t = index();
        assert_eq!(t.resolve_range("1s..2s"), Ok(30..60));
        assert_eq!(t.resolve_range("Intro"), Ok(0..100));
        assert_eq!(t.resolve_range("#2"), Ok(200..300));
        assert_eq!(t.resolve_range("Intro..#1"), Ok(0..200));
        assert_eq!(t.resolve_range("5s.."), Ok(150..300));
        assert_eq!(t.resolve_range("..1s"), Ok(0..30));
        assert_eq!(t.resolve_range("0..1000"), Ok(0..300));
        assert_eq!(t.resolve_range("all"), Ok(0..300));
        assert_eq!(t.resolve_range("42"), Ok(42..43));
        assert!(matches!(
            t.resolve_range("2s..1s"),
            Err(TimeSpecError::EmptyRange(_))
        ));
    }

    #[test]
    fn short_names() {
        assert_eq!(short_scene_name("a::b::Intro<'_>"), "Intro");
        assert_eq!(short_scene_name("Intro"), "Intro");
    }
}
