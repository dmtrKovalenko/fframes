use crate::{FFramesContext, ResolvedScenesTimeline, ScenesWithAudio, TimeBase, error};
use serde::Serialize;
use std::{
    iter::FromIterator,
    ops::{Add, Range, Sub},
    rc::Rc,
};

#[derive(Debug, Clone)]
pub enum AudioTimestamp<'a> {
    /// It is a **very specific** timestamp value. When used as end of audio range will be decoded as a whole duration of audio + start timestamp.
    /// So in case of range like `Second(10)..Eof` audio will be played from second 10 to the end of audio file.
    Eof,
    /// A flat frame within a video.
    Frame(usize),
    /// A second within a video timeline.
    Second(f32),
    Time {
        minutes: f32,
        seconds: f32,
    },
    /// Represents a flat duration of audio file. It is different from `Eof` in a way that it will always be decoded as a whole duration of audio file.
    /// So in case of range like `Second(10)..DurationOfAudio("audio20seconds.mp3")`, where audio20seconds's duration is 20 seconds, audio will be played only **10 seconds**.
    /// Because `DurationOfAudio` will always be resolved to 20 seconds.
    DurationOfAudio(&'a str),
    /// Represents a sum of two timestamps. Can be constructed using `+` operator.
    __Add(Rc<(AudioTimestamp<'a>, AudioTimestamp<'a>)>),
    /// Represents a subtraction of two timestamps. Can be constructed using `-` operator.
    __Subtract(Rc<(AudioTimestamp<'a>, AudioTimestamp<'a>)>),
}

impl<'a> Add for AudioTimestamp<'a> {
    type Output = AudioTimestamp<'a>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::__Add(Rc::new((self, rhs)))
    }
}

impl<'a> Sub for AudioTimestamp<'a> {
    type Output = AudioTimestamp<'a>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::__Subtract(Rc::new((self, rhs)))
    }
}

pub trait AudioTimelineUnit {
    fn from_frames(frames: usize, tb: &TimeBase) -> Self;
    /// Converts an exact timestamp. Frames are truncated like frame indexes, samples rounded.
    fn from_seconds(seconds: f64, tb: &TimeBase) -> Self;
    fn from_usize(val: usize) -> Self;
    fn as_usize(&self) -> usize;
    fn to_frames(&self, tb: &TimeBase) -> usize;
    fn to_seconds(&self, tb: &TimeBase) -> f64;
}

#[derive(PartialEq, PartialOrd, Debug, Copy, Clone)]
pub struct AudioTimelineFrames(usize);

impl AudioTimelineUnit for AudioTimelineFrames {
    fn from_frames(frames: usize, _: &TimeBase) -> Self {
        AudioTimelineFrames(frames)
    }
    fn from_seconds(seconds: f64, tb: &TimeBase) -> Self {
        AudioTimelineFrames(seconds_to_frames_floor(seconds, tb.fps))
    }
    fn as_usize(&self) -> usize {
        self.0
    }
    fn from_usize(val: usize) -> Self {
        AudioTimelineFrames(val)
    }
    fn to_frames(&self, _: &TimeBase) -> usize {
        self.0
    }
    fn to_seconds(&self, tb: &TimeBase) -> f64 {
        self.0 as f64 / tb.fps as f64
    }
}

#[derive(PartialEq, PartialOrd, Debug, Clone, Copy)]
pub struct AudioTimelineSamples(pub(crate) usize);

impl AudioTimelineUnit for AudioTimelineSamples {
    fn from_frames(frames: usize, tb: &TimeBase) -> Self {
        AudioTimelineSamples(frames * tb.sample_rate / tb.fps)
    }
    fn from_seconds(seconds: f64, tb: &TimeBase) -> Self {
        AudioTimelineSamples((seconds.max(0.) * tb.sample_rate as f64).round() as usize)
    }
    fn as_usize(&self) -> usize {
        self.0
    }
    fn from_usize(val: usize) -> Self {
        AudioTimelineSamples(val)
    }
    fn to_frames(&self, tb: &TimeBase) -> usize {
        self.0 * tb.fps / tb.sample_rate
    }
    fn to_seconds(&self, tb: &TimeBase) -> f64 {
        self.0 as f64 / tb.sample_rate as f64
    }
}

/// Seconds to whole frames, truncating like `(seconds * fps) as usize` but tolerant to
/// floating point error (`0.99999` frames of a whole second is still a whole frame).
pub(crate) fn seconds_to_frames_floor(seconds: f64, fps: usize) -> usize {
    (seconds.max(0.) * fps as f64 + 1e-6).floor() as usize
}

impl AudioTimestamp<'_> {
    fn infer_relying_on_dynamic_duration_audio_files<'a>(
        &self,
        name: &'a str,
    ) -> Option<Vec<&'a str>> {
        match self {
            AudioTimestamp::Eof => Some(vec![name]),
            AudioTimestamp::DurationOfAudio(_) => Some(vec![name]),
            AudioTimestamp::__Subtract(add) | AudioTimestamp::__Add(add) => {
                let (a, b) = &**add;
                let a = a.infer_relying_on_dynamic_duration_audio_files(name);
                let b = b.infer_relying_on_dynamic_duration_audio_files(name);

                match (a, b) {
                    (Some(mut a), Some(mut b)) => {
                        a.append(&mut b);
                        Some(a)
                    }
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            }
            AudioTimestamp::Frame(_) => None,
            AudioTimestamp::Second(_) => None,
            AudioTimestamp::Time { .. } => None,
        }
    }

    /// The timestamp in exact seconds. `eof_base` is added to the file duration for `Eof`
    /// (the start of the range minus the part of the file that is skipped).
    pub(crate) fn to_seconds(
        &self,
        filename: &str,
        fps: usize,
        eof_base: Option<f64>,
        resolve_audio_duration: &impl Fn(&str) -> error::Result<f64>,
    ) -> error::Result<f64> {
        Ok(match self {
            AudioTimestamp::Frame(frame) => *frame as f64 / fps as f64,
            AudioTimestamp::Second(seconds) => *seconds as f64,
            AudioTimestamp::Eof => resolve_audio_duration(filename)? + eof_base.unwrap_or(0.),
            AudioTimestamp::DurationOfAudio(filename) => resolve_audio_duration(filename)?,
            AudioTimestamp::Time { minutes, seconds } => *minutes as f64 * 60. + *seconds as f64,
            AudioTimestamp::__Add(add) => {
                let (a, b) = &**add;
                a.to_seconds(filename, fps, eof_base, resolve_audio_duration)?
                    + b.to_seconds(filename, fps, eof_base, resolve_audio_duration)?
            }
            AudioTimestamp::__Subtract(sub) => {
                let (a, b) = &**sub;
                a.to_seconds(filename, fps, eof_base, resolve_audio_duration)?
                    - b.to_seconds(filename, fps, eof_base, resolve_audio_duration)?
            }
        }
        .max(0.))
    }
}

type AudioDuration<'a> = Range<AudioTimestamp<'a>>;

/// Shape of a fade.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FadeCurve {
    /// Amplitude changes linearly. Good for very short de-click fades.
    Linear,
    /// `sin(t * PI / 2)`: constant power, the default for music fades and crossfades of
    /// different material.
    #[default]
    EqualPower,
    /// Raised cosine: gentle start and end.
    SCurve,
    /// Linear in decibels (-60 dB to 0 dB): perceptually even, for long music fade outs.
    Exponential,
}

impl FadeCurve {
    /// Gain for the fade progress `t` in `0..=1` (0 silent, 1 full level).
    pub fn gain(self, t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        match self {
            FadeCurve::Linear => t,
            FadeCurve::EqualPower => (t * std::f32::consts::FRAC_PI_2).sin(),
            FadeCurve::SCurve => (1. - (t * std::f32::consts::PI).cos()) * 0.5,
            FadeCurve::Exponential => {
                if t <= 0. {
                    0.
                } else {
                    10f32.powf(-60. * (1. - t) / 20.)
                }
            }
        }
    }
}

/// Lowers a track while voice tracks (see `AudioTrack::voice`) play. Driven by the timeline,
/// not the signal: the level drops before the voice starts and comes back after it ends.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Ducking {
    /// Attenuation while a voice plays, e.g. `-12.0`.
    pub depth_db: f32,
    /// Seconds before the voice starts over which the level goes down.
    pub attack: f32,
    /// Seconds the level stays down after the voice ends.
    pub hold: f32,
    /// Seconds over which the level comes back.
    pub release: f32,
    /// Voice ranges closer than this (seconds) are merged so the music does not pump
    /// between phrases.
    pub merge_gap: f32,
}

impl Default for Ducking {
    fn default() -> Self {
        Self {
            depth_db: -12.,
            attack: 0.2,
            hold: 0.3,
            release: 0.8,
            merge_gap: 0.8,
        }
    }
}

/// How a track is mixed. Everything defaults to "play the file as is".
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TrackMix {
    /// Level in decibels, `0.0` is the file's level.
    pub gain_db: f32,
    /// `-1.0` left .. `1.0` right. Mono files are centered at unity gain.
    pub pan: f32,
    /// Seconds.
    pub fade_in: f32,
    /// Seconds, ending at the end of the track's range.
    pub fade_out: f32,
    pub fade_curve: FadeCurve,
    /// Seconds of the file skipped before the track starts playing.
    pub offset: f32,
    /// Duck this track under voice tracks.
    pub duck: Option<Ducking>,
    /// This track is a voice: other tracks with `duck` get quieter while it plays.
    pub voice: bool,
}

impl Default for TrackMix {
    fn default() -> Self {
        Self {
            gain_db: 0.,
            pan: 0.,
            fade_in: 0.,
            fade_out: 0.,
            fade_curve: FadeCurve::default(),
            offset: 0.,
            duck: None,
            voice: false,
        }
    }
}

/// One audio file placed on the timeline, with its mix settings.
///
/// ```ignore
/// use fframes::{AudioTrack, AudioTimestamp::*};
///
/// AudioMap::from([
///     AudioTrack::new("music.mp3", Second(0.)..Eof).gain_db(-14.).fade_out(2.).duck_under_voice(),
///     AudioTrack::new("voice.wav", Second(1.5)..Eof).voice(),
///     AudioTrack::new("whoosh.wav", Second(4.2)..Eof).pan(-0.5),
/// ])
/// ```
#[derive(Debug, Clone)]
pub struct AudioTrack<'a> {
    pub file: &'a str,
    pub range: AudioDuration<'a>,
    pub mix: TrackMix,
}

impl<'a> AudioTrack<'a> {
    pub fn new(file: &'a str, range: AudioDuration<'a>) -> Self {
        Self {
            file,
            range,
            mix: TrackMix::default(),
        }
    }

    pub fn gain_db(mut self, gain_db: f32) -> Self {
        self.mix.gain_db = gain_db;
        self
    }

    /// Linear level, `0.5` is about -6 dB.
    pub fn volume(mut self, volume: f32) -> Self {
        self.mix.gain_db = 20. * volume.max(1e-6).log10();
        self
    }

    pub fn pan(mut self, pan: f32) -> Self {
        self.mix.pan = pan.clamp(-1., 1.);
        self
    }

    pub fn fade_in(mut self, seconds: f32) -> Self {
        self.mix.fade_in = seconds.max(0.);
        self
    }

    pub fn fade_out(mut self, seconds: f32) -> Self {
        self.mix.fade_out = seconds.max(0.);
        self
    }

    pub fn fade_curve(mut self, curve: FadeCurve) -> Self {
        self.mix.fade_curve = curve;
        self
    }

    /// Starts playing the file `seconds` into it.
    pub fn offset(mut self, seconds: f32) -> Self {
        self.mix.offset = seconds.max(0.);
        self
    }

    /// Marks the track as a voice that ducks tracks created with `duck_under_voice`.
    pub fn voice(mut self) -> Self {
        self.mix.voice = true;
        self
    }

    /// Lowers this track by 12 dB while voice tracks play.
    pub fn duck_under_voice(self) -> Self {
        self.duck(Ducking::default())
    }

    pub fn duck(mut self, ducking: Ducking) -> Self {
        self.mix.duck = Some(ducking);
        self
    }
}

impl<'a> From<(&'a str, AudioDuration<'a>)> for AudioTrack<'a> {
    fn from((file, range): (&'a str, AudioDuration<'a>)) -> Self {
        AudioTrack::new(file, range)
    }
}

#[derive(Debug, Clone)]
/// Audio map represents when and how long each audio file should be played within a video or a scene.
/// In case of scene audio map is always relative to the scene timestamp (which is resolved based on
/// the `Video::define_scenes()`).
///
/// @example
/// ```rust
/// use fframes::AudioTimestamp::*;
///
/// AudioMap::from([
///     (
///        "audio1.mp3",
///        Second(10)..Second(20)
///     )
///     (
///        "audio2.mp3",
///        Second(5)..Eof
///     )
/// ])
/// ```
///
/// In this case audio 1 will be playing from second 10 to second 20 and audio 2 will be playing from second 5 to the end of file.
/// Use `AudioTrack` entries instead of tuples for gain, pan, fades, offsets and ducking.
pub struct AudioMap<'a>(pub Option<Vec<AudioTrack<'a>>>);

/// A track with its position on the timeline resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAudioTrack<TUnit: AudioTimelineUnit> {
    pub file: String,
    pub range: Range<TUnit>,
    pub mix: TrackMix,
}

type AudioTimeline<TUnit> = Vec<ResolvedAudioTrack<TUnit>>;

/// The resolved audio_map contain each audio file position and duration in specified units.
#[derive(Debug)]
pub struct ResolvedAudioMap<TUnit: AudioTimelineUnit>(pub(crate) AudioTimeline<TUnit>);

impl<TUnit: AudioTimelineUnit + Copy> ResolvedAudioMap<TUnit> {
    pub(crate) fn round_max_duration(&mut self, max_duration: TUnit) {
        let max_duration_usize = max_duration.as_usize();

        for track in self.0.iter_mut() {
            if track.range.end.as_usize() > max_duration_usize {
                track.range.end = max_duration;
            }
        }
        // Tracks starting after the end are not played at all.
        self.0
            .retain(|track| track.range.start.as_usize() < max_duration_usize);
    }

    pub fn calc_stream_duration(&self) -> usize {
        self.0
            .iter()
            .map(|track| track.range.end.as_usize())
            .max()
            .unwrap_or(0)
    }

    pub fn tracks(&self) -> &[ResolvedAudioTrack<TUnit>] {
        &self.0
    }
}

impl<'a> AudioMap<'a> {
    /// Creates a new empty audio map. It means that no audio files will be played within the video/scene.
    /// If scene or video duration that defines this audio map uses `fframes::Duration::FromAudioMap` – rendering won't be possible.
    pub fn none() -> Self {
        AudioMap(None)
    }

    pub fn unstable_flatten_with_scenes(self, scene_audios: &'a ScenesWithAudio) -> Self {
        if self.0.is_none() && scene_audios.0.is_none() {
            return Self(None);
        }

        let mut map = self.0.unwrap_or_default();

        if let Some(scenes) = scene_audios.0.as_ref() {
            for audio_map in scenes.iter() {
                if let Some(scene_audio_map) = audio_map.audio_map.0.as_ref() {
                    map.extend(scene_audio_map.iter().cloned())
                }
            }
        }

        Self(Some(map))
    }

    pub fn used_audio_files<T: FromIterator<&'a str>>(&'a self) -> Option<T> {
        self.0.as_ref().map(|map| {
            map.iter()
                .filter_map(|AudioTrack { file, range, .. }| {
                    let start = range
                        .start
                        .infer_relying_on_dynamic_duration_audio_files(file);

                    let end = range
                        .end
                        .infer_relying_on_dynamic_duration_audio_files(file);

                    match (start, end) {
                        (Some(mut start), Some(mut end)) => {
                            start.append(&mut end);
                            Some(start)
                        }
                        (Some(start), None) => Some(start),
                        (None, Some(end)) => Some(end),
                        (None, None) => None,
                    }
                })
                .flatten()
                .collect::<T>()
        })
    }

    /// `resolve_audio_duration` returns the duration of a file in seconds.
    pub(crate) fn resolve<TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &self,
        offset: TUnit,
        tb: &TimeBase,
        resolve_audio_duration: impl Fn(&str) -> error::Result<f64>,
    ) -> error::Result<Option<AudioTimeline<TUnit>>> {
        let offset = offset.to_seconds(tb);
        self.0
            .as_ref()
            .map(|tracks| {
                tracks
                    .iter()
                    .map(|AudioTrack { file, range, mix }| {
                        let start =
                            range
                                .start
                                .to_seconds(file, tb.fps, None, &resolve_audio_duration)?;

                        let end = range.end.to_seconds(
                            file,
                            tb.fps,
                            Some(start - mix.offset as f64),
                            &resolve_audio_duration,
                        )?;

                        Ok(ResolvedAudioTrack {
                            file: file.to_string(),
                            range: TUnit::from_seconds(start + offset, tb)
                                ..TUnit::from_seconds(end.max(start) + offset, tb),
                            mix: *mix,
                        })
                    })
                    .collect::<error::Result<Vec<_>>>()
            })
            .transpose()
    }

    pub fn resolve_with_scenes<TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &'a self,
        scenes: Option<&ResolvedScenesTimeline>,
        tb: &TimeBase,
        resolve_audio_duration: impl Fn(&str) -> error::Result<f64>,
    ) -> error::Result<Option<ResolvedAudioMap<TUnit>>> {
        let global_resolved_map =
            self.resolve(TUnit::from_usize(0), tb, &resolve_audio_duration)?;

        let scenes_resolved_map = scenes
            .map(|scenes| -> crate::error::Result<_> {
                Ok(scenes
                    .iter()
                    .map(|(range, _, scene)| {
                        scene.audio().resolve(
                            TUnit::from_frames(range.start, tb),
                            tb,
                            &resolve_audio_duration,
                        )
                    })
                    .collect::<error::Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect::<Vec<_>>())
            })
            .transpose()?;

        Ok(match (scenes_resolved_map, global_resolved_map) {
            (Some(scenes_resolved_map), Some(mut global_resolved_map)) => {
                global_resolved_map.extend(scenes_resolved_map);
                Some(ResolvedAudioMap(global_resolved_map))
            }
            (Some(scene_maps), None) => Some(ResolvedAudioMap(scene_maps)),
            (None, Some(global_maps)) => Some(ResolvedAudioMap(global_maps)),
            (None, None) => None,
        })
    }

    pub fn resolve_with_ctx<'media: 'a, TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &'a self,
        ctx: &'a FFramesContext<'a, 'media>,
    ) -> error::Result<Option<ResolvedAudioMap<TUnit>>> {
        self.resolve_with_scenes(ctx.scenes, &ctx.time_base, |filename| {
            let audio_data = ctx.get_audio(filename).ok_or_else(|| {
                crate::error::FFramesError::RequiredAudioNotFound(filename.to_string())
            })?;
            Ok(audio_data.duration_in_seconds() as f64)
        })
    }
}

impl<'a, const N: usize> From<[(&'a str, AudioDuration<'a>); N]> for AudioMap<'a> {
    fn from(arr: [(&'a str, AudioDuration<'a>); N]) -> Self {
        AudioMap(Some(arr.into_iter().map(AudioTrack::from).collect()))
    }
}

impl<'a, const N: usize> From<[AudioTrack<'a>; N]> for AudioMap<'a> {
    fn from(arr: [AudioTrack<'a>; N]) -> Self {
        AudioMap(Some(arr.to_vec()))
    }
}

impl<'a> From<Vec<AudioTrack<'a>>> for AudioMap<'a> {
    fn from(tracks: Vec<AudioTrack<'a>>) -> Self {
        AudioMap(Some(tracks))
    }
}

impl<'a> FromIterator<(&'a str, AudioDuration<'a>)> for AudioMap<'a> {
    fn from_iter<T: IntoIterator<Item = (&'a str, AudioDuration<'a>)>>(iter: T) -> Self {
        AudioMap(Some(iter.into_iter().map(AudioTrack::from).collect()))
    }
}

impl<'a> FromIterator<AudioTrack<'a>> for AudioMap<'a> {
    fn from_iter<T: IntoIterator<Item = AudioTrack<'a>>>(iter: T) -> Self {
        AudioMap(Some(iter.into_iter().collect()))
    }
}
