# fframes now plays Lottie

An 18 second launch video for social feeds, 1080x1920 at 60 fps. Every animation in it is a
Lottie file from `media/`, played with `frame.play_lottie`. The music is synthesized in
`src/soundtrack.rs` when the video starts, so the example ships no audio files.

| beats | scene | what you see |
| --- | --- | --- |
| 0-8 | Hook | the wordmark and a party popper on the first frame, then "NOW PLAYS LOTTIE" by 1.3 s |
| 8-16 | Grid | twelve animations pop in on sixteenth notes and float |
| 16-24 | Code | the tiger plays above the `play_lottie` call that plays it |
| 24-28 | Zoom | the camera dives into the tiger's eye until the pupil fills the screen |
| 28-36 | Outro | the wordmark, `features = ["lottie"]` and fframes.studio |

`src/beat.rs` holds the timing at 120 BPM. The picture and the music both read it, so each pop
and keystroke has its sound on the same frame.

```sh
cargo run --release -p lottie-launch -- render                 # out.mp4, CPU backend
cargo run --release -p lottie-launch -- --renderer gpu render  # Skia on the GPU
```

The e2e suite (`just e2e cpu`) renders the tiger at four points of its loop and compares them
with `e2e/snapshots/lottie-launch`.

## Credits

- Emoji animations (`media/*.json` except the two below): [Noto Animated Emoji](https://googlefonts.github.io/noto-emoji-animation/) by Google, CC BY 4.0.
- `media/switch.json` and `media/watermelon.json`: [lottie-ios](https://github.com/airbnb/lottie-ios) samples, Apache 2.0.
- Fonts: Anton, IBM Plex Mono and Instrument Serif, SIL Open Font License.
