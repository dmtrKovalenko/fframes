use fframes_media_loaders::{FFramesSubtitles, FFramesSubtitlesCue};
use std::{ops::DerefMut, sync::Arc};
use usvgr::svgtree::SvgAttributeValue;

use crate::{
    BreaksLruCache, Svgr, SyncVideoFrameInput, VisualizeFrameInput, WrappedTextStructure,
    animation, get_visualization,
    text::{BreakLinesOpts, text_wrap_impl},
    video_data::{FFramesSyncedVideoFrame, VideoDecodersWorker},
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
    breaks_lru_cache: Option<BreaksLruCache>,
    worker_local_video_decoders: VideoDecodersWorker,
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
    pub fn new(index: usize, global_index: usize, fps: usize) -> Self {
        Self {
            index,
            global_index,
            fps,
            breaks_lru_cache: None,
            worker_local_video_decoders: VideoDecodersWorker::default(),
        }
    }

    /// This is an internal API used by the render to create frames during the rendering phase.
    /// It is not meant for public usage and might or might have breaking changes in any minor
    /// version change..
    ///
    /// Use at your own risk.
    pub fn __internal_make_for_renderer(
        index: usize,
        global_index: usize,
        fps: usize,
        breaks_lru_cache: Option<BreaksLruCache>,
        worker_local_decoders: VideoDecodersWorker,
    ) -> Self {
        Self {
            index,
            global_index,
            fps,
            breaks_lru_cache,
            worker_local_video_decoders: worker_local_decoders,
        }
    }

    /// Clones a frame but subtracts offset from the relative index for the resulting frame.
    /// Example: Scene frames are always shifting by the scene start frames.
    pub fn clone_with_scene_offset(frame: &Frame, offset: usize) -> Self {
        Self {
            index: frame.index - offset,
            global_index: frame.global_index + offset,
            fps: frame.fps,
            breaks_lru_cache: frame.breaks_lru_cache.clone(),
            worker_local_video_decoders: frame.worker_local_video_decoders.clone(),
        }
    }

    /// Borrows the frame into the same one, but removes relative index in favor of global one.
    /// Can be useful for using global-videos API from the scene.
    pub fn into_global(self) -> Self {
        Self {
            index: self.global_index,
            ..self
        }
    }

    /// Returns current frame timestamp in seconds.
    pub fn seconds(&self) -> f32 {
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

        match &self.seconds() {
            second if second < &on_second => from,
            second if second > &(on_second + duration) => to,
            second => {
                let progress = animation_runtime.solve(&(second - on_second));

                from.apply_progress(&to, progress)
            }
        }
    }

    fn animate_impl<T: crate::animation::Animatable + Copy + Default + std::fmt::Debug>(
        &self,
        animation: &animation::KeyFramesAnimation<T>,
        current_second: &f32,
    ) -> T {
        let keyframe = animation
            .keyframes
            .iter()
            .find(|keyframe| keyframe.seconds_range.contains(current_second));

        match keyframe {
            None => animation.final_value,
            Some(keyframe) => {
                let progress = keyframe
                    .animation_runtime
                    .solve(&(current_second - keyframe.seconds_range.start));

                keyframe.from.apply_progress(&keyframe.to, progress)
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
    ///         on 2.3, val 1400. => 770., Easing::Spring { mass: 1.85, stiffness: 130.0, damping: 16.0 },
    ///         on 4.8, val 770. => 1400., Easing::Spring { mass: 1.85, stiffness: 130.0, damping: 16.0 }
    ///     ))}
    ///   />
    /// );
    /// ```
    pub fn animate<T: crate::animation::Animatable + Copy + Default + std::fmt::Debug>(
        &self,
        animation: &animation::KeyFramesAnimation<T>,
    ) -> T {
        let current_second = &self.seconds();
        self.animate_impl(animation, current_second)
    }

    pub fn animate_loop<T: crate::animation::Animatable + Copy + Default + std::fmt::Debug>(
        &self,
        animation: &animation::KeyFramesAnimation<T>,
    ) -> T {
        let current_second = self.seconds() % animation.total_duration;
        self.animate_impl(animation, &current_second)
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
    /// It resolves individual character widths from provided font files automatically.
    /// **Important: it won't work with system fonts in the editor, but would work with them in the renderer**.
    ///
    /// Line breaking rules are compatible with CSS line breaking rules with not support for word
    /// break in any form.
    /// If font can not be resolved returns `None`.
    ///
    /// ```no_run
    /// let options = &fframes::BreakLinesOpts {
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
    /// let wrapped_text = frame.text_break_lines(&ctx, "Hello world", options);
    /// ```
    ///
    /// Output of this function can be converted to svgr using `WrappedTextStructure::as_svgr`
    ///
    /// ```no_run
    /// svgr!(
    ///    <g>
    ///        {wrapped_text.as_svgr(options)}
    ///    </g>
    /// )
    /// ```
    pub fn text_break_lines_structure<
        'a,
        X: Into<SvgAttributeValue<'a>> + std::hash::Hash + Default,
        Y: Into<SvgAttributeValue<'a>> + std::hash::Hash + Default,
    >(
        &mut self,
        ctx: &crate::FFramesContext<'a, '_>,
        value: &str,
        opts: BreakLinesOpts<'a, X, Y>,
    ) -> Option<WrappedTextStructure> {
        let font_source = ctx.font_source?;
        let hash = opts.hash_with_value(value);

        if let Some(cache_mutex) = self.breaks_lru_cache.as_ref() {
            cache_mutex
                .0
                .lock()
                .ok()?
                .deref_mut()
                .get_or_insert(hash, || text_wrap_impl(hash, value, font_source, opts))
                .clone()
        } else {
            text_wrap_impl(hash, value, font_source, opts)
        }
    }

    /// Wraps the text string into the lines according to the provided content area width.
    /// Outputs the ready to use svgr text element. If font can not be resolved returns `None`.
    ///
    /// It resolves individual character widths from provided font files automatically.
    /// **Important: it won't work with system fonts in the editor, but would work with them in the renderer**.
    ///
    /// ```no_run
    /// svgr!(
    ///   <g>
    ///     {frame.text_break_lines(&ctx, "Hello world", &fframes::BreakLinesOpts {
    ///         // max width of the text content. Once line become wider it wraps.
    ///         width: 500,
    ///         // resolved font family name (check preview media list)
    ///         font_family: "Roboto",
    ///         x: "100",
    ///         y: "100",
    ///         align: fframes::TextAlign::Center,
    ///         ..Default::default()
    ///     })}
    ///   </g>
    /// )
    /// ```
    ///
    /// This method is an alias to `text_break_lines_structure` method with `as_svgr` call.
    /// If you need more control or information about the text structure use
    /// `text_break_lines_structure` method.
    pub fn text_break_lines<
        'a,
        X: Into<SvgAttributeValue<'a>> + std::hash::Hash + Default + Copy,
        Y: Into<SvgAttributeValue<'a>> + std::hash::Hash + Default + Copy,
    >(
        &mut self,
        ctx: &crate::FFramesContext<'a, '_>,
        value: &str,
        opts: BreakLinesOpts<'a, X, Y>,
    ) -> Option<Svgr<'a>> {
        Some(
            self.text_break_lines_structure(ctx, value, opts)?
                .as_svgr(opts),
        )
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
        let milliseconds = (self.seconds() * 1000.0) as u64;

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
        let milliseconds = (self.seconds() * 1000.0) as u64;

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
        let milliseconds = (self.seconds() * 1000.0) as u64;

        subtitles.get_cue_stack(milliseconds, overlap)
    }

    /// Returns a synced video frame of the video file media.
    /// If the video file is not found or there is no frame or error during decoding returns None.
    /// Video frame is decoded and timebase (fps) is synced with the target fframe's video frame.
    ///
    /// **Make sure this function is not working in the editor for now. Editor will automatically fallback to
    /// `{your_video_file}.{your_extension}_fallback.{jpg|png}` image file for preview if it exists
    pub fn get_synced_video_frame<'media>(
        &self,
        ctx: &crate::FFramesContext<'_, 'media>,
        file_name: impl AsRef<str>,
        input: &SyncVideoFrameInput<'media>,
    ) -> Option<Arc<impl FFramesSyncedVideoFrame<'media> + 'media>> {
        let video = ctx.get_video(file_name.as_ref())?;
        let start_from_frame = self.second_to_frame(input.start_from);
        if self.index < start_from_frame {
            return None;
        }

        let offset = self.index - start_from_frame;
        self.worker_local_video_decoders
            .get_synced_frame(video, offset as i64, ctx, input)
            .map_err(|e| {
                crate::log!("Error while decoding video frame: {:?}", e);
            })
            .ok()
            .flatten()
    }
}
