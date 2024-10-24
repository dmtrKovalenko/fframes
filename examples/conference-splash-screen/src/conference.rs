use crate::sponsor_screen::SponsorScene;
use crate::{media::ConferenceMedia, SpeakerScene};
use fframes::{AudioMap, FFramesContext, Frame, Scene, Svgr};
pub use fframes::{Color, Video};

#[derive(Debug, Clone)]
pub struct ConferenceVideo<'a> {
    pub media: &'a ConferenceMedia,
    pub sponsor_scene: SponsorScene<'a>,
    pub speaker_scene: SpeakerScene<'a>,
}

impl Video for ConferenceVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::FromAudio("track.mp3")
    }

    fn audio(&self) -> AudioMap {
        use fframes::AudioTimestamp::*;

        AudioMap::from([("track.mp3", (Frame(0)..Eof))])
    }

    fn define_scenes(&self) -> fframes::Scenes {
        let vec: Vec<&dyn Scene> = vec![&self.sponsor_scene, &self.speaker_scene];

        fframes::Scenes::from(vec)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            {ctx.render_scenes(&frame)}
          </svg>
        )
    }
}
