# Subtitles and captions

Use this for subtitles, lyrics and any typography driven by speech. Other videos do not
need a caption track.

## The data

A caption track is a WebVTT file or a JSON file in `media/`. `CaptionTrack::parse` detects
the format from the content; both become the same owned `CaptionTrack` (`Subtitles` is an
alias). Use VTT when you only have phrases. Use JSON when you have word timings or want
per-word settings:

```json
{
  "cues": [
    {
      "start": 0, "end": 2,
      "words": [
        {"text": "Choose", "start": 0, "end": 0.7},
        {"text": "the", "start": 0.7, "end": 0.9},
        {"text": "Moon", "start": 0.9, "end": 1.5,
         "metadata": {"effect": "underline", "color": "#ffd166"}},
        {"text": ".", "start": 1.5, "end": 1.6, "space_before": false}
      ]
    },
    {"start": 2.4, "end": 4, "text": "A phrase without word timing.", "speaker": "B"}
  ]
}
```

- Times are seconds on one clock; a cue or word is visible over `[start, end)`.
- Word times are absolute, not relative to the cue. Words are ordered, do not overlap and
  lie inside their cue; the cue may start earlier or end later to hold the phrase on screen.
- A cue has either `text` or `words`. Cues are ordered by start and may overlap (two
  speakers). Optional `id`, `speaker` and `metadata` on cues, `id`, `space_before` and
  `metadata` on words. `metadata` is any JSON object; the parser keeps it untouched.
- Do not fabricate word timings by splitting a phrase evenly. Use timings from a
  transcription that outputs word timestamps, or show the whole phrase.
- Invalid timing, ordering or duplicate ids fail at load with the path of the field
  (`cues[3].words[1].end`), not at render time.

Loading is the same as for any other media: `include_media_dir!` embeds the file,
`MediaDirectory::read_folder` reads it at runtime, the editor loads it over HTTP. JSON
whose top-level object has `cues` is a caption track; any other JSON is not.
`ctx.get_subtitles("speech.json")` returns the `&CaptionTrack`: `cues()`,
`active_cues(seconds)`, and on each `CaptionCue` `phrase()`, `words`, `metadata`.
A track can also be built in code: `CaptionTrack::new([CaptionCue::words(0.0, 2.0,
[CaptionWord::new("Choose", 0.0, 0.7), ..]), CaptionCue::text(2.4, 4.0, "..")])`.

For a plain phrase caption `frame.get_subtitle_phrase(track)` returns the current cue
text; this older helper includes the cue's end timestamp, `Captions` and `active_cues`
do not.

## Rendering: `Captions` (feature `captions`)

`fframes::captions::Captions` lays out a track once and draws the visible words of any
frame. Build it in the video constructor and keep it on the video:

```rust
use fframes::captions::{CaptionClock, CaptionMode, CaptionRect, Captions};

let captions = Captions::from_media("speech.json", CaptionRect::new(120.0, 760.0, 1680.0, 240.0))
    .font("DM Sans", 64)        // family and size, measured from the loaded font
    .font_weight(600)
    .line_height(1.25)
    .mode(CaptionMode::Highlight)
    .active_fill("#ffd166").upcoming_fill("#aeb4c0").spoken_fill("#ffffff")
    .outline("#000000", 3.0)    // stroke behind the glyphs
    .active_background("#1b1f2a", 10.0) // rounded box behind the active word, padding
    .underline("#ffd166", 5.0); // bar under the active word
```

`Captions::new(track, rect)` takes an owned `CaptionTrack` instead of a media name.
The rectangle is in composition pixels. Each phrase is wrapped to its width, lines are
centred, and the block is anchored to the bottom edge of the rectangle; simultaneous cues
are stacked in order. Nothing moves when the active word changes.

In `render_frame` insert `{self.captions.render(&mut frame, ctx)}`. `render` resolves the
track from the media provider, lays it out on first use (and again if the provider gets a
new track under that name), and returns the SVG for the frame. Missing font, a word wider
than the rectangle, or more lines than fit are reported as `inspect` errors and the captions
are left out of that frame; `try_render` returns them as `CaptionError` instead.

Modes:

- `Highlight` (default): the whole phrase, each word coloured by state
  (`upcoming_fill`, `active_fill`, `spoken_fill`).
- `Karaoke`: like Highlight, the active word fills left to right with its progress.
- `Reveal`: words appear as they are spoken and stay; upcoming words are hidden.
- `Word`: only the active word, centred in the rectangle; nothing during gaps.

A VTT phrase has no word timing: every mode shows the whole phrase in `spoken_fill`.

Clock: by default the caption time is the scene-local frame time, like animations. Use
`.clock(CaptionClock::Global)` for a transcript of the whole video inside scenes, or
`render_at(seconds, &mut frame, ctx)` when you map the time yourself (a clip that starts
later, plays faster, or is cut).

## Custom word drawing

`highlight_with` runs a callback for every active timed word, `render_with` for every
visible word. The callback gets a borrowed `CaptionWordScope` and the owned preset SVG of
that word, and returns the SVG to draw. Coordinates are local to the word: `(0, 0)` is the
top-left of its advance box, `scope.width()` × `scope.height()`. The component translates
the result into place, nothing is clipped, so decorations may extend outside the box.

```rust
#[derive(serde::Deserialize)]
#[serde(tag = "effect", rename_all = "kebab-case")]
enum Highlight {
    Underline { color: String },
    Fill { color: String },
}

let captions = self.captions.highlight_with(&mut frame, ctx, |scope, text| {
    match scope.word.metadata.typed::<Highlight>() {
        Some(Highlight::Underline { color }) => fframes::svgr!(
            <g>{text}
                <rect x="0" y={scope.height() - 4.0} width={scope.width()} height="4"
                    fill={color.as_str()} /></g>
        ),
        Some(Highlight::Fill { color }) => scope.mask_text(fframes::svgr!(
            <rect width={scope.width()} height={scope.height()} fill={color.as_str()} />
        )),
        None => text, // keep the preset drawing
    }
});
```

What the scope offers:

- `scope.word`: `text`, `state` (`Upcoming | Active | Spoken`), `progress` (0..=1 of the
  word's duration), `start`/`end` (`None` for a VTT phrase), `metadata`, `cue_index`,
  `word_index`. `scope.cue_metadata` is the cue's object. `scope.is_active()`.
- `scope.time` (caption seconds), `scope.elapsed()` (seconds since the word started),
  `scope.frame` (the `Frame`, for `frame.animate` or `Shader::draw`).
- `scope.composition_rect`: where the word lands on the canvas, if something else must
  align with it.
- `scope.mask_text(svg)`: show `svg` only through the word's glyphs (gradient or shader
  fills). `scope.clip(svg)`: clip to the advance box. `scope.id("key")`: an SVG id that is
  unique per word and frame, for `<defs>` the callback creates.
- `render_at_with` and `highlight_at_with` take explicit seconds; every variant has a
  `try_` form that returns the error.

For a layout of your own, `captions.words_at(seconds, &mut frame, ctx)` returns the
positioned words (`rect` in composition pixels, state, progress, metadata) and you draw
them yourself.

## Metadata

Describe per-word or per-cue settings as a Serde type, preferably an enum with one variant
per effect and the parameters inside the variant. Defaults live in Rust; JSON carries only
the choices. `metadata.typed::<T>()` decodes the object as `T` the first time it is asked
for and caches the result (also a failed decode) for that object, shared by every frame,
thread and clone of the track. Nothing is registered up front and no JSON is parsed per
frame. It returns `None` when the object is empty or does not match `T`; if a setting is
required, check it in the constructor and report the cue and word. `metadata.get("key")`
reads the raw JSON value.

## Rules that keep it fast

- Load the track once through the media provider; never call `CaptionTrack::parse`,
  `serde_json` or `metadata.parse::<T>()` (the uncached decode) in `render_frame`.
- One `Captions` per treatment, built once. Changing font or layout settings means
  building a new one. Layout and font measurements are cached inside it.
- Build `Shader`s and any glyph textures once; change only uniforms per frame.
- Drawing costs about a microsecond per visible word; the size of the track does not matter.

## Check it

`$R inspect` reports caption layout errors (font not loaded, word wider than the box, too
many lines, stacked cues taller than the box). Then look: `$R frame 1.4s,2.3s` at word
boundaries, a gap between cues, and a frame where two cues overlap, and `$R strip <cue
range> -n 8` to see the highlight move. Check that the lowest line clears the safe area
and that the active colour reads against the real background.
