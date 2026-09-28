# Sound for fframes videos

You cannot hear the result. Place sound from the timeline, check it with the numbers below,
and ask the user to listen in `cargo run --release -- preview`.

## Files

- Short SFX and music can live in `media/` (embedded, decoded to mono at compile time; mp3,
  wav, flac, aac, ogg, m4a).
- Stereo music, voice-overs and anything large: a folder loaded at runtime keeps stereo:

```rust
let audio_dir = fframes::MediaDirectory::read_folder("audio")?;
let audio = audio_dir.process_media_source()?;
let media = fframes::CombinedMediaProvider::from([&static_media as &dyn fframes::MediaProvider, &audio]);
// RenderOptions { media: Some(&media), .. }
```

- Generating placeholder sounds without assets: `sox` (`sox -n pop.wav synth 0.08 sine 880 fade 0 0.08 0.07`)
  or `ffmpeg -f lavfi -i "sine=frequency=440:duration=0.2" tick.wav`. Tell the user they are
  placeholders.

## Placing tracks

```rust
use fframes::{AudioMap, AudioTrack, AudioTimestamp::*, Ducking, FadeCurve};

fn audio(&self) -> AudioMap<'_> {
    AudioMap::from([
        AudioTrack::new("music.mp3", Second(0.)..Eof).gain_db(-18.).fade_in(1.5).fade_out(2.5)
            .fade_curve(FadeCurve::EqualPower).duck_under_voice(),
        AudioTrack::new("vo_01.wav", Second(0.6)..Eof).voice(),
        AudioTrack::new("vo_02.wav", Second(6.2)..Eof).voice(),
        AudioTrack::new("whoosh.wav", Second(TRANSITION_AT - 0.15)..Eof).gain_db(-10.),
        AudioTrack::new("pop.wav", Second(ITEM_AT + 0.05)..Eof).gain_db(-12.).pan(0.3),
        AudioTrack::new("take.wav", Second(20.)..Second(24.)).offset(3.2),  // file 3.2s..7.2s
    ])
}
```

- Timestamps: `Second(f32)` (sample accurate), `Frame(n)`, `Time { minutes, seconds }`,
  `Eof` as the end (= play to the end of the file), `DurationOfAudio("f")`, and `+`/`-`.
- Scenes can have their own `fn audio` with times relative to the scene start: sounds move
  with the scene when durations change.
- Derive SFX times from the same constants that drive the animation. Whooshes start ~150 ms
  before the movement peak; pops/clicks land on the frame the element appears (+0-50 ms).
- Gain guidelines relative to a normalised voice at 0 dB: music under voice -16 to -22 dB
  (plus ducking), music alone -8 to -12 dB, UI SFX -8 to -14 dB, whooshes -8 to -12 dB.
- `duck_under_voice()` = -12 dB, 0.2 s attack before the voice, 0.3 s hold, 0.8 s release,
  voice gaps under 0.8 s merged. `.duck(Ducking { depth_db: -15., ..Default::default() })` to tune.
- The master bus sums linearly and limits peaks at -1 dBFS (`RenderOptions::audio_mix`), so
  loud overlaps do not distort, but a limiter working hard squashes the mix: fix gains rather
  than relying on it.

## Checking without listening

```bash
$R timeline                         # every track with its seconds, gain, fades, voice/ducked
$R audio at 3.1s,6.2s               # what plays at an event, position in the file, level, ducking
$R audio analyze                    # loudness report, per scene
$R audio analyze --waveform w.png   # read the image: waveform, loudness curve, scene lines, cue ticks
$R audio render Intro -o intro.wav  # the mix of one scene, if the user wants to listen
```

Targets for the report:
- Integrated loudness: about -14 LUFS for web/social (-16 for podcasts, -23 for broadcast).
  More than 2 LU off: adjust gains (all tracks) or `audio_mix.master_gain_db`.
- True peak at or below -1 dBTP; `clipped_samples` 0.
- Loudness range 4-10 LU for music videos and explainers; far higher means quiet and loud
  parts need balancing.
- `silent_ranges` only where silence is intended (an unexpected one means a track ends early
  or a file is missing: check `missing_files` and `timeline`).
- Per scene: scenes with voice should be within ~2 LU of each other.

The waveform image shows cue ticks (yellow) under the scene lines (purple): each sound should
sit where its visual event is. For a precise check use `audio at` on the event time and a
`frame` of the same time.
