//! Named cue points on a video timeline.
//!
//! Declare them on [`Video::markers`](crate::Video::markers). They show up in `timeline`, can be
//! used as time specs (`title-hit`, `title-hit..cta`) and are always included when `inspect`
//! samples frames. Look them up from `render_frame` with [`FFramesContext::marker`](crate::FFramesContext::marker).
use crate::audio_map::AudioMap;
use crate::duration::Duration;
use crate::error::{FFramesError, Result};
use serde::Serialize;
use std::iter::FromIterator;

/// A named moment on the video timeline, in the same duration units as scenes and audio.
///
/// ```
/// # use fframes::{Duration, Marker};
/// let hit = Marker::new("title-hit", Duration::Seconds(1.8));
/// assert_eq!(hit.name, "title-hit");
/// ```
#[derive(Debug)]
pub struct Marker<'a> {
    pub name: &'a str,
    pub at: Duration<'a>,
}

impl<'a> Marker<'a> {
    pub fn new(name: &'a str, at: Duration<'a>) -> Self {
        Self { name, at }
    }
}

/// The list returned by [`Video::markers`](crate::Video::markers). Empty by default.
#[derive(Debug, Default)]
pub struct Markers<'a>(pub Vec<Marker<'a>>);

impl Markers<'_> {
    pub fn none() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Marker<'_>> {
        self.0.iter()
    }

    pub fn used_audio_files(&self) -> Vec<&str> {
        self.0
            .iter()
            .filter_map(|marker| marker.at.used_audio_files())
            .flatten()
            .collect()
    }

    pub fn used_video_files(&self) -> Vec<&str> {
        self.0
            .iter()
            .filter_map(|marker| marker.at.used_video_files())
            .flatten()
            .collect()
    }
}

impl<'a> From<Vec<Marker<'a>>> for Markers<'a> {
    fn from(markers: Vec<Marker<'a>>) -> Self {
        Self(markers)
    }
}

impl<'a, const N: usize> From<[Marker<'a>; N]> for Markers<'a> {
    fn from(markers: [Marker<'a>; N]) -> Self {
        Self(markers.into())
    }
}

impl<'a, const N: usize> From<[(&'a str, Duration<'a>); N]> for Markers<'a> {
    fn from(markers: [(&'a str, Duration<'a>); N]) -> Self {
        Self(
            markers
                .into_iter()
                .map(|(name, at)| Marker::new(name, at))
                .collect(),
        )
    }
}

impl<'a> FromIterator<Marker<'a>> for Markers<'a> {
    fn from_iter<T: IntoIterator<Item = Marker<'a>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a> FromIterator<(&'a str, Duration<'a>)> for Markers<'a> {
    fn from_iter<T: IntoIterator<Item = (&'a str, Duration<'a>)>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .map(|(name, at)| Marker::new(name, at))
                .collect(),
        )
    }
}

/// A marker after the video duration has been resolved to frames.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimelineMarker {
    pub name: String,
    pub frame: usize,
    pub seconds: f32,
}

pub(crate) fn normalize_marker_name(name: &str) -> String {
    name.replace(['_', '-', ' '], "").to_lowercase()
}

pub(crate) fn resolve_markers(
    markers: &Markers<'_>,
    fps: usize,
    audio_map: &AudioMap,
    resolve_media_duration: &impl Fn(&str) -> Result<f64>,
    duration_in_frames: usize,
) -> Result<Vec<TimelineMarker>> {
    let last_frame = duration_in_frames.saturating_sub(1);
    let mut resolved = Vec::with_capacity(markers.0.len());

    for marker in &markers.0 {
        let name = marker.name.trim();
        if name.is_empty() {
            return Err(FFramesError::InvalidMarker(
                "a timeline marker is missing a name".to_owned(),
            ));
        }

        let frame = marker
            .at
            .to_frames_async(fps, audio_map, resolve_media_duration)?
            .min(last_frame);

        let key = normalize_marker_name(name);
        if resolved
            .iter()
            .any(|existing: &TimelineMarker| normalize_marker_name(&existing.name) == key)
        {
            return Err(FFramesError::InvalidMarker(format!(
                "marker \"{name}\" is defined more than once"
            )));
        }

        resolved.push(TimelineMarker {
            name: name.to_owned(),
            frame,
            seconds: frame as f32 / fps as f32,
        });
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioMap;

    fn resolve(markers: Markers<'_>) -> Result<Vec<TimelineMarker>> {
        resolve_markers(&markers, 30, &AudioMap::none(), &|_| Ok(4.0), 300)
    }

    #[test]
    fn seconds_and_frames_become_timeline_points() {
        let markers = resolve(Markers::from([
            ("title-hit", Duration::Seconds(1.2)),
            ("end-card", Duration::Frames(240)),
        ]))
        .unwrap();

        assert_eq!(markers[0].name, "title-hit");
        assert_eq!(markers[0].frame, 36);
        assert_eq!(markers[0].seconds, 1.2);
        assert_eq!(markers[1].frame, 240);
        assert_eq!(markers[1].seconds, 8.0);
    }

    #[test]
    fn past_the_end_clamps_to_the_last_frame() {
        let markers = resolve(Markers::from([("late", Duration::Seconds(20.0))])).unwrap();
        assert_eq!(markers[0].frame, 299);
    }

    #[test]
    fn duplicate_names_are_an_error() {
        let err = resolve(Markers::from([
            ("Title Hit", Duration::Seconds(1.0)),
            ("title-hit", Duration::Seconds(2.0)),
        ]))
        .unwrap_err();
        assert!(
            matches!(err, FFramesError::InvalidMarker(message) if message.contains("title-hit"))
        );
    }

    #[test]
    fn empty_names_are_an_error() {
        let err = resolve(Markers::from([("  ", Duration::Seconds(1.0))])).unwrap_err();
        assert!(matches!(err, FFramesError::InvalidMarker(_)));
    }

    #[test]
    fn from_audio_uses_the_resolver() {
        let markers = Markers::from([("sting", Duration::FromAudio("hit.wav"))]);
        let resolved = resolve_markers(
            &markers,
            10,
            &AudioMap::none(),
            &|name| {
                assert_eq!(name, "hit.wav");
                Ok(1.5)
            },
            100,
        )
        .unwrap();
        assert_eq!(resolved[0].frame, 15);
    }
}
