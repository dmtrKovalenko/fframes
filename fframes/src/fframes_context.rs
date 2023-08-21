use crate::{
    media, AudioData, AudioTimelineSamples, AudioTimelineUnit, DynamicMediaProvider, FontSource,
    Frame, MediaProvider, ResolvedAudioMap, ResolvedScenesTimeline, Svgr,
};
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
pub struct FFramesContext<'a> {
    pub time_base: TimeBase,
    pub mode: FFramesMode,
    pub media_provider: &'a DynamicMediaProvider<'a>,
    pub duration_in_frames: usize,
    pub font_source: Option<&'a (dyn FontSource<'a> + 'a)>,
    pub scenes: Option<&'a ResolvedScenesTimeline>,
}

impl<'a: 'b, 'b> FFramesContext<'a> {
    pub fn get_audio_data(&self, filename: &str) -> &AudioData {
        todo!()
    }

    pub fn get_subtitles(&self, filename: impl AsRef<str>) -> &'b media::Subtitles {
        let filename = filename.as_ref();
        match self.media_provider.subtitles.get(filename) {
            Some(data) => data,
            None => panic!(
                "Subtitles {file} not found! Please make sure that media folder contains {file}",
                file = filename
            ),
        }
    }

    pub fn get_image_link(&self, filename: &str) -> String {
        "todo remove me".to_owned()
    }

    pub fn render_scenes(&self, global_frame: &Frame) -> Svgr {
        if let Some(scenes) = self.scenes.as_ref() {
            Svgr::from_iter(scenes.0.iter().filter_map(|(range, _, scene)| {
                range.contains(&global_frame.index).then(|| {
                    scene.render_frame(
                        Frame {
                            fps: global_frame.fps,
                            global_index: global_frame.index,
                            index: global_frame.index - range.start,
                            breaks_lru_cache: global_frame.breaks_lru_cache.clone(),
                        },
                        self,
                    )
                })
            }))
        } else {
            Svgr::default()
        }
    }

    /// Finds the scene layout and duration information based on the layout of defined in `define_scenes` of the `Video`.
    pub fn get_scene_info<T: crate::Scene>(&self, scene: &T) -> Option<&crate::SceneInfo> {
        if let Some(scenes) = self.scenes.as_ref() {
            scenes.0.iter().find_map(|(_, info, boxed_scene)| {
                #[allow(clippy::ptr_eq)]
                let pointers_equal = boxed_scene.as_ref() as *const dyn crate::Scene as *const T
                    == scene as *const T;

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

        audio_map.0.iter().for_each(|(f, sample_range)| {
            if sample_range.contains(&start_sample) {
                let start_of_this_frame_in_file =
                    start_sample.as_usize() - sample_range.start.as_usize();

                self.get_audio_data(f)
                    .get_range(
                        start_of_this_frame_in_file..start_of_this_frame_in_file + frame_size,
                    )
                    .map(|data| {
                        data.iter().enumerate().for_each(|(i, sample)| {
                            let fltp_sample = *sample as f32 / i16::MAX as f32;
                            let filled_sample = audio_data[i];

                            if filled_sample == 0. {
                                audio_data[i] = fltp_sample
                            } else {
                                audio_data[i] =
                                    filled_sample + fltp_sample - (filled_sample * fltp_sample)
                            }
                        });
                    });
            }
        });

        audio_data
    }
}
