//! A part of a video file played inside a scene.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use crate::media::ImageData;
use crate::video_data::FFramesSyncedVideoFrame;
use crate::{
    Duration, FFramesContext, Frame, FrameConvertOptions, PlaybackSpeed, ResizeVideoFrame,
};

/// Added to the length of a clip before it is cut to whole frames, so a length like
/// `24.7 - 19.2` that float math makes a bit shorter than 5.5 s does not lose a frame.
const FRAME_EPSILON_SECONDS: f64 = 1e-4;

/// A part of a video file: the seconds `source` of `video`, played at `speed` from the
/// second `at` of the scene that shows it.
///
/// A clip has no position on the timeline, so it moves with its scene when an earlier
/// scene gets shorter. Put the sound that belongs to the clip (the voice recorded with
/// it) into the scene's `audio` at the same second.
///
/// [`Clip::image`] gives the frame of the current frame as an image. Draw it like any
/// other image, with your own size, crop, transform, filter, mask or shader:
///
/// ```rust,ignore
/// use fframes::{
///     AudioMap, AudioTimestamp::Second, AudioTrack, Clip, Duration, FFramesContext, Frame,
///     Scene, Svgr,
/// };
///
/// /// The DJI recording starts 3.650375 s before the camera file.
/// const DJI_AFTER_CAMERA: f64 = 3.650375;
///
/// #[derive(Debug)]
/// struct Hook<'a> {
///     face: Clip<'a>,
///     plan: Clip<'a>,
/// }
///
/// impl Scene for Hook<'_> {
///     fn duration(&self) -> Duration<'_> {
///         self.face.duration()
///     }
///
///     fn audio(&self) -> AudioMap<'_> {
///         AudioMap::from([AudioTrack::new(
///             "dji_13.wav",
///             Second(0.)..Second(self.face.seconds() as f32),
///         )
///         .offset((self.face.source.start + DJI_AFTER_CAMERA) as f32)])
///     }
///
///     fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
///         let Some(face) = self.face.image(&frame, ctx) else { return Svgr::empty() };
///         // decoded at the size it is shown
///         let plan = self.plan.image_at_width(&frame, ctx, 440);
///         fframes::svgr!(
///             <g>
///                 <image href={face.href()} x="0" y="0" width="1080" height="1920"
///                        preserveAspectRatio="xMidYMid slice" />
///                 {plan.map(|plan| fframes::svgr!(
///                     <image href={plan.href()} x="580" y="1180" width="440" height="660"
///                            preserveAspectRatio="xMidYMid slice" clip-path="url(#rounded)" />
///                 ))}
///             </g>
///         )
///     }
/// }
///
/// let hook = Hook {
///     face: Clip::new("IMG_0926.mov", 19.2..24.7),
///     // a screen recording from 1.2 s of the scene
///     plan: Clip::new("screen.mov", 3.0..6.0).speed(1.5).at(1.2),
/// };
/// ```
///
/// Each clip decodes with its own decoder, so a clip and a picture in picture of the
/// same file do not slow each other down. A reversed clip seeks back on every frame:
/// with long keyframe intervals (phone footage) it is slow to render.
#[derive(Debug, Clone)]
pub struct Clip<'a> {
    /// The video file, a name of the media folder.
    pub video: &'a str,
    /// The seconds of the file the clip plays, `start` included, `end` not.
    pub source: Range<f64>,
    /// The second of the scene when the clip starts. Default `0.0`.
    pub at: f32,
    /// `2.0` plays the clip twice as fast (and half as long), `0.5` at half speed.
    /// Kept to 1/1000 like [`PlaybackSpeed`]. Default `1.0`.
    pub speed: f32,
    /// Plays the clip backwards, from the end of `source` to its start.
    pub reverse: bool,
    /// Starts the clip again after its end, for as long as the scene shows it.
    pub looping: bool,
    /// The image the editor shows until the frame is decoded.
    pub editor_fallback_image: Option<&'a ImageData<'a>>,
}

impl<'a> Clip<'a> {
    /// The seconds `source` of `video` at normal speed, from the start of the scene.
    pub fn new(video: &'a str, source: Range<f64>) -> Self {
        Self {
            video,
            source,
            at: 0.0,
            speed: 1.0,
            reverse: false,
            looping: false,
            editor_fallback_image: None,
        }
    }

    #[must_use]
    pub fn at(mut self, seconds: f32) -> Self {
        self.at = seconds;
        self
    }

    #[must_use]
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    #[must_use]
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    #[must_use]
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    #[must_use]
    pub fn editor_fallback_image(mut self, image: Option<&'a ImageData<'a>>) -> Self {
        self.editor_fallback_image = image;
        self
    }

    fn speed_value(&self) -> f64 {
        f64::from(PlaybackSpeed::new(self.speed).thousandths()) / f64::from(PlaybackSpeed::SCALE)
    }

    fn source_seconds_len(&self) -> f64 {
        (self.source.end - self.source.start).max(0.0)
    }

    /// The length of the clip on the timeline in seconds: the source length divided by
    /// the speed.
    pub fn seconds(&self) -> f64 {
        self.source_seconds_len() / self.speed_value()
    }

    /// The length of the clip, for the `duration` of the scene that shows it. It does not
    /// include `at`.
    pub fn duration(&self) -> Duration<'static> {
        Duration::Seconds((self.seconds() + FRAME_EPSILON_SECONDS) as f32)
    }

    /// The number of frames the clip plays at `fps`, the same as [`Clip::duration`].
    pub fn frames(&self, fps: usize) -> usize {
        ((self.seconds() + FRAME_EPSILON_SECONDS) * fps as f64) as usize
    }

    /// The second of the file shown on frame `index` of the clip (0 is the first frame of
    /// the clip, at `at`). `None` after the end of the clip unless it loops.
    pub fn source_seconds(&self, index: usize, fps: usize) -> Option<f64> {
        let frames = self.frames(fps);
        if frames == 0 || (index >= frames && !self.looping) {
            return None;
        }

        let step = self.speed_value() / fps as f64;
        let played = (index % frames) as f64 * step;
        let seconds = if self.reverse {
            (self.source.end - step - played).max(self.source.start)
        } else {
            self.source.start + played
        };
        Some(seconds)
    }

    /// The decoded frame of the clip on `frame` of the scene, with its size and timestamp.
    /// `None` before `at`, after the end, or when the file is missing or fails to decode.
    pub fn frame(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
    ) -> Option<Arc<impl FFramesSyncedVideoFrame<'a> + use<'a>>> {
        let start_frame = (f64::from(self.at) * frame.fps as f64).round() as usize;
        let index = frame.index.checked_sub(start_frame)?;
        let seconds = self.source_seconds(index, frame.fps)?;
        frame.video_frame_at_keyed(
            ctx,
            self.video,
            seconds,
            self.decoder_key(),
            self.editor_fallback_image,
        )
    }

    /// The clip on `frame` of the scene as an image at the size of the video file, for
    /// `<image href={image.href()} ... />` or a shader. `None` when [`Clip::frame`] is.
    pub fn image(&self, frame: &Frame, ctx: &FFramesContext<'_, '_>) -> Option<ImageData<'a>> {
        Some(self.frame(frame, ctx)?.into_image())
    }

    /// [`Clip::image`] decoded at `width` pixels, the height follows the aspect ratio of
    /// the file. A 4K clip shown small (a picture in picture) converts faster this way.
    pub fn image_at_width(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
        width: u32,
    ) -> Option<ImageData<'a>> {
        let video_frame = self.frame(frame, ctx)?;
        let height = (f64::from(width) * f64::from(video_frame.height())
            / f64::from(video_frame.width().max(1)))
        .round() as u32;
        video_frame.into_resized_image(&FrameConvertOptions {
            resize: ResizeVideoFrame {
                width: width.max(1),
                height: height.max(1),
            },
        })
    }

    /// Clips that play the same thing share a decoder, other clips never do.
    fn decoder_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.video.hash(&mut hasher);
        self.source.start.to_bits().hash(&mut hasher);
        self.source.end.to_bits().hash(&mut hasher);
        PlaybackSpeed::new(self.speed)
            .thousandths()
            .hash(&mut hasher);
        self.reverse.hash(&mut hasher);
        // 0 is the key of `Frame::video_frame_at`
        hasher.finish().max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: usize = 30;

    fn sources(clip: &Clip, indexes: Range<usize>) -> Vec<Option<f64>> {
        indexes
            .map(|i| clip.source_seconds(i, FPS).map(|s| (s * 1e6).round() / 1e6))
            .collect()
    }

    #[test]
    fn float_math_does_not_cost_a_frame() {
        assert_eq!(Clip::new("a.mp4", 19.2..24.7).frames(FPS), 165);
        assert_eq!(Clip::new("a.mp4", 0.7..1.4).frames(FPS), 21);
        assert_eq!(Clip::new("a.mp4", 3.0..9.4).speed(1.25).frames(FPS), 153);
    }

    #[test]
    fn duration_resolves_to_the_same_frames() {
        for clip in [
            Clip::new("a.mp4", 19.2..24.7),
            Clip::new("a.mp4", 0.7..1.4),
            Clip::new("a.mp4", 3.0..9.4).speed(1.25),
        ] {
            let resolved = clip
                .duration()
                .to_frames_async(FPS, &crate::AudioMap::none(), &|_| Ok(0.0))
                .unwrap();
            assert_eq!(resolved, clip.frames(FPS));
        }
    }

    #[test]
    fn plays_the_source_range_from_its_start() {
        let clip = Clip::new("a.mp4", 19.2..19.3);
        assert_eq!(
            sources(&clip, 0..4),
            [Some(19.2), Some(19.233_333), Some(19.266_667), None]
        );
    }

    #[test]
    fn speed_scales_the_source_time() {
        let clip = Clip::new("a.mp4", 1.0..2.0).speed(2.0);
        assert_eq!(clip.frames(FPS), 15);
        assert_eq!(
            sources(&clip, 0..3),
            [Some(1.0), Some(1.066_667), Some(1.133_333)]
        );
        assert_eq!(
            clip.source_seconds(14, FPS)
                .map(|s| (s * 1e6).round() / 1e6),
            Some(1.933_333)
        );
        assert_eq!(clip.source_seconds(15, FPS), None);
    }

    #[test]
    fn reverse_plays_the_same_frames_backwards() {
        let forward = Clip::new("a.mp4", 1.0..1.2);
        let backward = forward.clone().reverse();
        let mut expected = sources(&forward, 0..6);
        expected.reverse();
        assert_eq!(sources(&backward, 0..6), expected);
        assert_eq!(backward.source_seconds(6, FPS), None);
    }

    #[test]
    fn looping_starts_again_after_the_end() {
        let clip = Clip::new("a.mp4", 1.0..1.1).looping();
        assert_eq!(
            sources(&clip, 0..5),
            [
                Some(1.0),
                Some(1.033_333),
                Some(1.066_667),
                Some(1.0),
                Some(1.033_333)
            ]
        );
    }

    #[test]
    fn an_empty_range_shows_nothing() {
        assert_eq!(Clip::new("a.mp4", 2.0..2.0).source_seconds(0, FPS), None);
        assert_eq!(
            Clip::new("a.mp4", 2.0..1.0)
                .looping()
                .source_seconds(0, FPS),
            None
        );
    }

    #[test]
    fn clips_of_one_file_get_their_own_decoder() {
        let face = Clip::new("a.mp4", 0.0..5.0);
        assert_eq!(face.decoder_key(), face.clone().decoder_key());
        assert_ne!(
            face.decoder_key(),
            Clip::new("a.mp4", 5.0..9.0).decoder_key()
        );
        assert_ne!(face.decoder_key(), face.clone().speed(2.0).decoder_key());
        assert_ne!(face.decoder_key(), face.clone().reverse().decoder_key());
    }
}
