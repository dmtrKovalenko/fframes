//! A/V sync check: a click track beeps at every full second (higher pitch on every 4th
//! beat) while the circle flashes and the hand points straight up at the same moment.
//!
//! `cargo run --release -p fframes_native_player --example audio_sync`
use fframes::{
    AudioMap, AudioTimestamp, Color, FFramesContext, Frame, StaticMediaProvider, Svgr, Video,
    include_media_dir,
};

include_media_dir!(pub struct AudioSyncMedia, "fframes-native-player/examples/media");

const CLICK_TRACK: &str = "click_track.mp3";

#[derive(Debug)]
struct AudioSync;

impl Video for AudioSync {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::rgba(15, 23, 42, 255);

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::FromAudio(CLICK_TRACK)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([(CLICK_TRACK, AudioTimestamp::Second(0.)..AudioTimestamp::Eof)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let seconds = frame.seconds();
        let beat = seconds as usize % 4;
        let phase = seconds.fract();
        // Full brightness exactly on the beep, fading out over a quarter of a second.
        let flash = (1. - phase / 0.25).max(0.);
        let accent = if beat == 0 { "#f43f5e" } else { "#38bdf8" };

        let beat_markers = (0..4)
            .map(|index| {
                let fill = if index == beat { accent } else { "#334155" };
                let x = 780 + index * 100;
                fframes::svgr!(<rect x={x} y="900" width="60" height="60" rx="12" fill={fill} />)
            })
            .collect::<Vec<_>>();

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080"
                 width={Self::WIDTH} height={Self::HEIGHT}>
                <circle cx="960" cy="480" r="320" fill="none" stroke="#334155" stroke-width="12" />
                <circle cx="960" cy="480" r="300" fill={accent} opacity={flash} />
                // one revolution per second: points straight up when the beep starts
                <g transform={format!("rotate({} 960 480)", phase * 360.)}>
                    <rect x="952" y="170" width="16" height="310" rx="8" fill="#f8fafc" />
                </g>
                <circle cx="960" cy="480" r="24" fill="#f8fafc" />
                {beat_markers}
                <text x="960" y="1040" font-size="44" text-anchor="middle" fill="#94a3b8">
                    {format!("{seconds:05.2}s  frame {}", frame.index)}
                </text>
            </svg>
        )
    }
}

fn main() {
    let media = AudioSyncMedia::prepare().expect("static media");

    fframes_native_player::play(
        &AudioSync,
        &fframes_native_player::PlayerOptions {
            media: Some(&media),
            // only the frame counter uses text, the system default font is fine for it
            load_system_fonts: true,
            title: "audio-sync",
            ..Default::default()
        },
    )
    .unwrap();
}
