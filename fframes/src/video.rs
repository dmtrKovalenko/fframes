use crate::audio_map::AudioMap;
use crate::error::Result;
use crate::{
    AudioTimelineUnit, Color, Duration, FFramesContext, Frame, ResolvedAudioMap, SceneInfo, Svgr,
    TimeBase, scenes::*,
};

/// The base fframes video trait. It represents how to render a video for a struct which becomes an
/// input of the video.
pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;

    /// Background color of the video. This allows to set a solid color background.
    ///
    /// Make sure if you want to render a transparent video use `Color::TRANSPARENT` here
    /// **and** set the proper encoder and `pixel_format` that supports transparency
    /// (e.g. encoder libx265 with yuva420p pixel format) when rendering the video.
    const BACKGROUND_COLOR: Color = Color::BLACK;

    /// Defines either dynamic or inferred duration of the video
    fn duration(&self) -> Duration<'_>;

    /// Defines the audio timeline of the video (when and how long audio tracks are played)
    fn audio(&self) -> AudioMap<'_>;

    /// Defines the scenes timeline of the video.
    /// Each scene is an dyn object which implements the `Scene` trait.
    ///
    /// Every scene must be either bound to the `&self` lifetime or be a zero sized type.
    /// In short: put your scenes to the `&self` or do not add any fields to the scene struct.
    ///
    /// # Example
    /// ```no_run
    /// use fframes::{Scene, Scenes, Video};
    ///
    /// #[derive(Debug)]
    /// struct Intro;
    /// #[derive(Debug)]
    /// struct Chapter {
    ///     title: String,
    /// }
    ///
    /// impl Scene for Intro { /* duration, render_frame */ }
    /// impl Scene for Chapter { /* duration, render_frame */ }
    ///
    /// struct MyVideo {
    ///     chapter: Chapter,
    /// }
    ///
    /// impl Video for MyVideo {
    ///     fn define_scenes(&self) -> Scenes<'_> {
    ///         let scenes: Vec<&dyn Scene> = vec![
    ///             // a zero sized type, so the reference can be created right here
    ///             &Intro,
    ///             // a scene with data is borrowed from `self`
    ///             &self.chapter,
    ///         ];
    ///         Scenes::from(scenes)
    ///     }
    /// }
    /// ```
    fn define_scenes(&self) -> Scenes<'_> {
        Scenes(None)
    }

    /// This function is going to be called for each frame of the video and expects to return
    /// a valid SVG rendering tree for the specific frame.
    ///
    /// This function is going to be called thousands of times per rendering, so it is important to reduce
    /// amount of allocations and cpu bound operations happening during the render frame. It is
    /// possible to cache the data in the `self` and use it in the function or to memoize the data
    /// in `self` using `once_cell::LazyLock` or similar constructs.
    ///
    /// **Tip:** Avoid panicking in this function as much as possible, this function does not
    /// return `Result` because it is extremely expensive to stop the rendering once it has
    /// started. Prepare compiler guaranteed data in advance and read it from `self`.
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a>;
}

#[derive(Debug, Clone)]
pub struct ResolvedScenesTimeline<'a> {
    pub total_scenes_duration: usize,
    pub(crate) timeline: Vec<(std::ops::Range<usize>, SceneInfo, &'a (dyn Scene + 'a))>,
}

impl<'a> ResolvedScenesTimeline<'a> {
    /// The frame after the last frame of every scene, in timeline order. With overlapping
    /// scenes this is where the previous scene has left the screen. Renderers start
    /// their encoded segments there when they can, so a scene's video clip is decoded
    /// from its first frames instead of from a seek into it.
    pub fn scene_ends(&self) -> Vec<usize> {
        self.timeline
            .iter()
            .map(|(range, _, _)| range.end)
            .collect()
    }

    pub(crate) fn iter(
        &'a self,
    ) -> impl Iterator<Item = &'a (std::ops::Range<usize>, SceneInfo, &'a (dyn Scene + 'a))> {
        self.timeline.iter()
    }

    fn from_scenes(
        time_base: &TimeBase,
        scenes: &[SceneWithAudio<'a>],
        resolve_audio_duration: &impl Fn(&str) -> super::error::Result<f64>,
    ) -> Result<Self> {
        let scenes_count = scenes.len();
        let mut final_duration = 0;
        let mut resolved_scenes = Vec::new();

        for (index, SceneWithAudio { scene, audio_map }) in scenes.iter().enumerate() {
            let duration = scene.duration().to_frames_async(
                time_base.fps,
                audio_map,
                &resolve_audio_duration,
            )?;

            let (overlap_prev, overlap_next) = scene.overlap().to_frames(time_base.fps);
            let start_frame = final_duration - overlap_prev;
            let end_frame = final_duration + duration + overlap_next;
            resolved_scenes.push((
                start_frame..end_frame,
                SceneInfo {
                    index,
                    start_frame,
                    end_frame,
                    total_scenes_in_video: scenes_count,
                    duration_in_frames: duration,
                    is_last: index == scenes_count - 1,
                },
                *scene,
            ));

            // the scene duration is reuded by any overlap but we should be careful with the scenes
            // that have the overlap larger than the scene duration itself
            final_duration = (final_duration + duration).saturating_sub(overlap_next);
        }

        Ok(ResolvedScenesTimeline {
            total_scenes_duration: final_duration,
            timeline: resolved_scenes,
        })
    }
}

pub struct ResolvedRenderingTimeline<'a, TAudioUnit: AudioTimelineUnit + std::fmt::Debug> {
    pub audio_map: Option<ResolvedAudioMap<TAudioUnit>>,
    pub scenes: Option<ResolvedScenesTimeline<'a>>,
    pub duration_in_frames: usize,
}

pub fn resolve_timeline<
    'a,
    TAudioUnit: AudioTimelineUnit + std::fmt::Debug + Copy,
    TFun: Fn(&str) -> super::error::Result<f64>,
>(
    duration: &Duration,
    scenes: &ScenesWithAudio<'a>,
    time_base: &TimeBase,
    top_level_audio_map: &AudioMap,
    resolve_audio_duration: TFun,
) -> Result<ResolvedRenderingTimeline<'a, TAudioUnit>> {
    let (duration, resolved_scenes) = match (scenes.0.as_deref(), duration) {
        (Some(scenes), Duration::Auto) => {
            let timeline =
                ResolvedScenesTimeline::from_scenes(time_base, scenes, &resolve_audio_duration)?;

            (timeline.total_scenes_duration, Some(timeline))
        }
        // if both duration and scenes provided we use duration
        (Some(scenes), duration) => {
            let timeline =
                ResolvedScenesTimeline::from_scenes(time_base, scenes, &resolve_audio_duration)?;
            let total_duration = duration.to_frames_async(
                time_base.fps,
                top_level_audio_map,
                &resolve_audio_duration,
            )?;

            (total_duration, Some(timeline))
        }
        (None, duration) => {
            let total_duration = duration.to_frames_async(
                time_base.fps,
                top_level_audio_map,
                &resolve_audio_duration,
            )?;

            (total_duration, None)
        }
    };

    let mut resolved_audio_map = top_level_audio_map.resolve_with_scenes::<TAudioUnit>(
        resolved_scenes.as_ref(),
        time_base,
        &resolve_audio_duration,
    )?;

    if let Some(resolved_audio_map) = resolved_audio_map.as_mut() {
        resolved_audio_map.round_max_duration(TAudioUnit::from_frames(duration, time_base));
    }

    Ok(ResolvedRenderingTimeline {
        audio_map: resolved_audio_map,
        scenes: resolved_scenes,
        duration_in_frames: duration,
    })
}
