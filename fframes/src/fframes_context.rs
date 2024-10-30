use crate::media::{ImageData, Subtitles};
use crate::{
    AudioData, AudioTimelineSamples, AudioTimelineUnit, FontSource, Frame, MediaProvider,
    ResolvedAudioMap, ResolvedScenesTimeline, Svgr,
};
use fframes_media_loaders::VideoMedia;
use std::iter::FromIterator;

#[derive(Clone, Debug)]
pub enum FFramesMode {
    Editor,
    EditorTimelinePreview,
    Renderer,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeBase {
    pub fps: usize,
    pub sample_rate: usize,
}

#[derive(Debug)]
pub struct FFramesContext<'a, 'media: 'a> {
    pub time_base: TimeBase,
    pub duration_in_frames: usize,
    pub width: usize,
    pub height: usize,
    pub fps: usize,
    pub mode: FFramesMode,
    pub media_source: Option<&'media (dyn MediaProvider<'media>)>,
    pub font_source: Option<&'a (dyn FontSource<'a> + 'a)>,
    pub scenes: Option<&'a ResolvedScenesTimeline<'a>>,
}

impl<'a, 'media: 'a> FFramesContext<'a, 'media> {
    pub fn get_audio(&self, filename: impl AsRef<str>) -> Option<&'media AudioData<'media>> {
        self.media_source?.resolve_audio(filename.as_ref())
    }

    pub fn get_subtitles(&self, filename: impl AsRef<str>) -> Option<&'media Subtitles<'media>> {
        self.media_source?.resolve_subtitles(filename.as_ref())
    }

    pub fn get_image(&self, filename: impl AsRef<str>) -> Option<&'media ImageData> {
        self.media_source?.resolve_image(filename.as_ref())
    }

    pub fn get_video(&self, filename: impl AsRef<str>) -> Option<&'media VideoMedia> {
        self.media_source?.resolve_video(filename.as_ref())
    }

    pub fn render_scenes(&self, global_frame: &Frame) -> Svgr<'a> {
        if let Some(scenes) = self.scenes.as_ref() {
            Svgr::from_iter(
                scenes
                    .iter()
                    .filter(|&(range, _, _scene)| range.contains(&global_frame.index))
                    .map(|(range, _, scene)| {
                        scene.render_frame(
                            Frame::clone_with_scene_offset(global_frame, range.start),
                            self,
                        )
                    }),
            )
        } else {
            Svgr::default()
        }
    }

    /// Finds the scene layout and duration information based on the layout of defined in `define_scenes` of the `Video`.
    pub fn get_scene_info<T: crate::Scene>(&self, scene: &T) -> Option<&crate::SceneInfo> {
        if let Some(scenes) = self.scenes.as_ref() {
            scenes.iter().find_map(|(_, info, boxed_scene)| {
                #[allow(clippy::ptr_eq)]
                let pointers_equal =
                    *boxed_scene as *const dyn crate::Scene as *const T == scene as *const T;

                pointers_equal.then_some(info)
            })
        } else {
            None
        }
    }

    /// This is internal method that is used by the renderer which mixes audio data and returns the final as fltp in a vector.
    #[allow(clippy::option_map_unit_fn)]
    pub fn get_mixed_audio_data_in_fltp(
        &self,
        audio_map: &ResolvedAudioMap<AudioTimelineSamples>,
        start_sample: AudioTimelineSamples,
        frame_size: usize,
    ) -> Vec<f32> {
        let mut audio_data = vec![0.0; frame_size];

        let media_source = if let Some(media_source) = self.media_source {
            media_source
        } else {
            return vec![];
        };

        audio_map.0.iter().for_each(|(f, sample_range)| {
            if sample_range.contains(&start_sample) {
                let start_of_this_frame_in_file =
                    start_sample.as_usize() - sample_range.start.as_usize();

                let range = media_source.resolve_audio(f).and_then(|a| {
                    a.get_range(
                        start_of_this_frame_in_file..start_of_this_frame_in_file + frame_size,
                    )
                });

                if let Some(range) = range {
                    range.iter().enumerate().for_each(|(i, sample)| {
                        let filled_sample = audio_data[i];

                        if filled_sample == 0. {
                            audio_data[i] = *sample
                        } else {
                            audio_data[i] = filled_sample + *sample - (filled_sample * *sample)
                        }
                    });
                }
            }
        });

        audio_data
    }
}
