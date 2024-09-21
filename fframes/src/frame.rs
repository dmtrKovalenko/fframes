use fframes_media_loaders::{FFramesSubtitles, FFramesSubtitlesCue};
use std::ops::DerefMut;

use crate::{
    animation, get_visualization,
    text_wrap::{text_wrap_impl, BreakLinesOpts},
    BreaksLruCache, VisualizeFrameInput, WrappedTextStructure,
};

/// Contains all the temporal information about the current frame and the mutable links to the
/// intermidient caches.
#[derive(Debug, Default, Clone)]
pub struct Frame {
    /// The frame index of the current scene. If rendering a Scene it is relative to the current frame.
    pub index: usize,
    /// The frame index of the current frame. If rendering within a scene will show the frame index within a whole video.
    /// If rendering without scene always equal to self.index.
    pub global_index: usize,
    /// FPS of the video. Always equals to the FPS constant.
    pub fps: usize,
    pub breaks_lru_cache: Option<BreaksLruCache>,
}

pub struct AnimateRuntimeInput<'a, TValue: animation::Animatable> {
    /// The second when animation should start.
    /// If you want to animate based on frames then use `frame.animate_runtime(AnimateRuntimeInput { on_second: frame.frame_to_second(10), .. })`
    ///
    /// If current frame is before this second then `from` value will be returned.
    pub on_second: f32,
    /// The value to animate from.
    pub from: TValue,
    /// The value to animate to.
    pub to: TValue,
    /// The easing function to use.
    pub animation_runtime: &'a animation::AnimationRuntime,
}

impl Frame {
    /// Borrows the frame into the same one, but removes relative index in favor of global one.
    /// Can be useful for using global-videos API from the scene.
    pub fn into_global(self) -> Self {
        Self {
            index: self.global_index,
            ..self
        }
    }

    /// Returns current frame timestamp in seconds.
    pub fn get_current_second(&self) -> f32 {
        self.frame_to_second(self.index)
    }

    /// Converts frame index to second.
    pub fn frame_to_second(&self, frame: usize) -> f32 {
        frame as f32 / self.fps as f32
    }

    /// Converts a second value to a frame index within current scene.
    pub fn second_to_frame(&self, second: f32) -> usize {
        (second * self.fps as f32) as usize
    }

    /// Calculates animation in runtime.
    /// Unlike frame.animate(fframes::timeline!()) you can pass dynamic value in the start/from/to properties.
    /// But it is required to prepare easing function in advance.
    ///
    /// Use frame.animate! if possible.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fframes::{animation, AnimationRuntime, AnimateRuntimeInput, Frame};
    ///
    /// let frame = Frame { index: 0, global_index: 0, fps: 60, ..Default::default() };
    /// let runtime = AnimationRuntime::from_easing(&animation::Easing::Linear(2.0));
    ///
    /// let value = frame.animate_runtime(AnimateRuntimeInput {  on: 3.2, from: 1000., to: 2000., animation_runtime: &runtime }); assert_eq!(value, 1000.);
    /// ```
    pub fn animate_runtime<TValue: animation::Animatable>(
        &self,
        AnimateRuntimeInput {
            on_second,
            from,
            to,
            animation_runtime,
        }: AnimateRuntimeInput<TValue>,
    ) -> TValue {
        let duration = animation_runtime.get_duration();

        match &self.get_current_second() {
            second if second < &on_second => from,
            second if second > &(on_second + duration) => to,
            second => {
                let progress = animation_runtime.solve(&(second - on_second));

                from.apply_progress(&to, progress)
            }
        }
    }

    /// Returns the current value of the animation at the current second.
    ///
    /// Timeline is defined using fframes::timeline! macros. All the gaps between frames are filled automatically.
    ///
    /// ## Example
    ///
    /// In this example we have the 2 transition and 5 states. Value based on seconds:
    /// * from 0 to 2.3 -> 1400
    /// * from 2.3 to 2.9 -> transition from 1400 to 770 (duration calculates based on spring duration)
    /// * from 2.9 to 4.8 -> 770
    /// * from 4.8 to 5.4 -> transition from 770 to 1400 (duration calculates based on spring duration)
    /// * from 5.4 to end of file -> 1400
    ///
    /// ```no_run
    /// use crate::fframes::{ Frame, svgr, Svgr, animation::Easing};
    /// let frame = fframes::Frame { index: 0, global_index: 0, fps: 60, ..Default::default() };
    ///
    /// fframes::svgr!(
    ///   <rect
    ///     y={frame.animate(fframes::timeline!(
    ///         on 2.3, val 1400. => 770., Easing::Spring2(1.85, 130.0, 16.0),
    ///         on 4.8, val 770. => 1400., Easing::Spring2(1.85, 130.0, 16.0)
    ///     ))}
    ///   />
    /// );
    /// ```
    pub fn animate<T: crate::animation::Animatable + Copy + Default>(
        &self,
        animation: &animation::KeyFramesAnimation<T>,
    ) -> T {
        let current_second = &self.get_current_second();

        let keyframe = animation
            .keyframes
            .iter()
            .rev()
            .find(|keyframe| keyframe.seconds_range.contains(current_second));

        match keyframe {
            None => T::default(),
            Some(keyframe) => {
                let progress = keyframe
                    .animation_runtime
                    .solve(&(current_second - keyframe.seconds_range.start));

                keyframe.from.apply_progress(&keyframe.to, progress)
            }
        }
    }

    pub fn visualize_audio_frame(&self, input: VisualizeFrameInput) -> Vec<f32> {
        if self.index < input.smooth_level * 2 + 1 {
            return get_visualization(self.index, self.fps, &input);
        }

        let frames_to_smooth = ((self.index - input.smooth_level)
            ..(self.index + input.smooth_level))
            .map(|i| get_visualization(i, self.fps, &input))
            .collect::<Vec<_>>();

        (0..frames_to_smooth[1].len())
            .map(|frame| {
                frames_to_smooth.iter().map(|arr| arr[frame]).sum::<f32>()
                    / frames_to_smooth.len() as f32
            })
            .collect()
    }

    /// Wraps the text string into the lines according to the provided content area width.
    /// This function is executed in runtime and create idiomatic svg <text> <tspan> text </tspan> </text> structure
    /// which wraps the text string into the lines.
    ///
    /// It resolves individual character widths from provided font files automatically. **Important:
    /// it won't work with system fonts in the editor, but would work with them in the renderer**.
    ///
    /// Line wrap rules is the same as css have for standard (no break-word support) wrapping. If font can not be resolved returns `None`.
    ///
    /// @example
    /// ```no_run
    /// let frame = fframes::Frame {
    ///   ..Default::default()
    /// };
    ///
    /// let ctx: fframes::FFramesContext = todo!();
    ///
    /// let wrapped_text = frame.text_break_lines(&ctx, "Hello world", &fframes::BreakLinesOpts {
    ///     // max width of the text content. Once line become wider it wraps.
    ///     width: 500,
    ///     // font family name resolved. Can be checked in the editor for resolved font file.
    ///     font_family: "Roboto",
    ///     // the x position of the text element
    ///     x: "100",
    ///     // the y position of the text element
    ///     y: "100",
    ///     align: fframes::TextAlign::Center,
    ///     ..Default::default()
    /// });
    ///
    /// ```  
    pub fn text_break_lines<'a>(
        &mut self,
        ctx: &crate::FFramesContext<'a, '_>,
        value: &str,
        opts: BreakLinesOpts<'a>,
    ) -> Option<crate::Svgr<'a>> {
        let font_source = ctx.font_source?;
        let hash = opts.hash_with_value(value);

        if let Some(cache_mutex) = self.breaks_lru_cache.as_ref() {
            cache_mutex
                .0
                .lock()
                .ok()?
                .deref_mut()
                .get_or_insert(hash, || {
                    text_wrap_impl(value, font_source, opts)
                        .map(|lines| WrappedTextStructure::new(lines, hash))
                })
                .as_ref()
                .map(|structure| structure.as_svgr(opts))
        } else {
            text_wrap_impl(value, ctx.font_source?, opts)
                .map(|lines| WrappedTextStructure::new(lines, hash).as_svgr(opts))
        }
    }

    /// Same as `text_break_lines` but returns inner lines structure instead of ready-to-render svgr.
    /// It may be used to customize renderer of wrapped text lines. Every line contains `dx` and `dy` fields which must
    /// be passed to `dx={line.dx} dy={line.dy}` attribute of the every line <tspan> element.
    pub fn text_break_lines_structure<'a: 'b, 'b, 'media: 'a>(
        &mut self,
        ctx: &crate::FFramesContext<'a, 'media>,
        value: &'b str,
        opts: &BreakLinesOpts,
    ) -> Option<WrappedTextStructure> {
        let font_source = ctx.font_source?;
        let hash = opts.hash_with_value(value);

        if let Some(cache_mutex) = self.breaks_lru_cache.as_ref() {
            cache_mutex
                .0
                .lock()
                .ok()?
                .deref_mut()
                .get_or_insert(hash, || {
                    text_wrap_impl(value, font_source, *opts)
                        .map(|lines| WrappedTextStructure::new(lines, hash))
                })
                .clone()
        } else {
            text_wrap_impl(value, font_source, *opts)
                .map(|lines| WrappedTextStructure::new(lines, hash))
        }
    }

    /// Returns a phrase that must be rendered by time in this frame.
    /// If there is no phrase to render returns None.
    ///
    /// # Examples
    /// ```no_run
    ///  let frame = fframes::Frame {
    ///     ..Default::default()
    ///  };
    ///
    ///  let phrase = frame.get_subtitle_phrase(&subtitles);
    pub fn get_subtitle_phrase<'a>(
        &self,
        subtitles: &'a impl FFramesSubtitles<'a>,
    ) -> Option<&'a str> {
        let milliseconds = (self.get_current_second() * 1000.0) as u64;

        let (_, cue) = subtitles.get_cue_by_time(milliseconds)?;
        Some(cue.text())
    }

    /// Returns a cue that must be rendered by the time of the current frame
    /// Besides text cue contains additional metadata like start/end time stamp,
    /// notes and cue settings which can be used to customise text.
    ///
    /// Read more about available data and cue setting at https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API
    pub fn get_subtitle_cue<'a, TSubtitles: FFramesSubtitles<'a>>(
        &self,
        subtitles: &'a TSubtitles,
    ) -> Option<&'a TSubtitles::Cue> {
        let milliseconds = (self.get_current_second() * 1000.0) as u64;

        subtitles.get_cue_by_time(milliseconds).map(|(_, cue)| cue)
    }

    /// Returns all the cues in order which timestamp is before or equal current frame.
    ///
    /// ### Params
    /// * `overlap` – value in milliseconds is used to control when the next cue will join the stack,
    ///   e.g if overlap is 1000ms, the next cue will join the stack when it's timestamp is 1000ms or less
    ///   then the time of the current frame.
    pub fn get_cue_stack<'a, TSubtitles: FFramesSubtitles<'a>>(
        &self,
        subtitles: &'a TSubtitles,
        overlap: u64,
    ) -> Vec<&'a TSubtitles::Cue> {
        let milliseconds = (self.get_current_second() * 1000.0) as u64;

        subtitles.get_cue_stack(milliseconds, overlap)
    }
}
