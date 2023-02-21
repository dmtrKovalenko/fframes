use std::{
    collections::hash_map::DefaultHasher,
    fmt::Debug,
    hash::{Hash, Hasher},
    ops::DerefMut,
};

use crate::{
    animation, get_visualization,
    text_wrap::{text_wrap_impl, BreakLinesOpts},
    AnimationRuntime, BreaksLruCache, VisualizeFrameInput,
};

/// The Frame {} struct contains temporal information about the current frame.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// The frame index of the current scene. If rendering a Scene it is relative to the current frame.
    pub index: usize,
    /// The frame index of the current frame. If rendering within a scene will show the frame index within a whole video.
    /// If rendering without scene always equal to self.index.
    pub global_index: usize,
    /// FPS of the video. Always equals to the Video::FPS constant.
    pub fps: usize,
    pub breaks_lru_cache: Option<BreaksLruCache>,
}

pub struct AnimateRuntimeInput<'a> {
    pub on: f32,
    pub from: f32,
    pub to: f32,
    pub animation_runtime: &'a AnimationRuntime,
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

    pub fn get_current_second(&self) -> f32 {
        self.index as f32 / self.fps as f32
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
    pub fn animate_runtime(
        &self,
        AnimateRuntimeInput {
            on,
            from,
            to,
            animation_runtime,
        }: AnimateRuntimeInput,
    ) -> f32 {
        let duration = animation_runtime.get_duration();

        match &self.get_current_second() {
            second if second < &on => from,
            second if second > &(on + duration) => to,
            second => {
                let progress = animation_runtime.solve(&(second - on));

                let animation_range = to - from;
                from + animation_range * progress
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
    pub fn animate<T: crate::Animatable + Copy + Default>(
        &self,
        animation: &animation::SteppedAnimation<T>,
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
            .into_iter()
            .map(|i| get_visualization(i, self.fps, &input))
            .collect::<Vec<_>>();

        (0..frames_to_smooth[1].len())
            .into_iter()
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
        ctx: &crate::FFramesContext<'a>,
        value: &'a str,
        opts: &BreakLinesOpts,
    ) -> Option<crate::Svgr> {
        let font_source = ctx.font_source?;
        let mut s = DefaultHasher::new();
        value.hash(&mut s);
        opts.hash(&mut s);
        let hash = s.finish();

        if let Some(cache_mutex) = self.breaks_lru_cache.as_ref() {
            cache_mutex
                .0
                .lock()
                .ok()?
                .deref_mut()
                .get_or_insert(hash, || text_wrap_impl(value, hash, font_source, *opts))
                .clone()
        } else {
            text_wrap_impl(value, hash, ctx.font_source?, *opts)
        }
    }
}
