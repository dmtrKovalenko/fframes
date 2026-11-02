# Subtitles and reusable word effects

Use this reference for subtitles, lyrics or typography driven by speech. Motion graphics
and 3D videos do not require a caption document. Start with the small working example in
[`examples/captions`](https://github.com/dmtrKovalenko/fframes/tree/main/examples/captions):
one JSON track feeds Highlight, Karaoke, Reveal and Word modes, plus a custom SVG highlight.

## Data and timing

Load subtitles, fonts, video and audio through the normal FFrames media provider.
`CaptionTrack::parse` accepts WebVTT or ordinary JSON. `Subtitles` is a compatibility alias
for `CaptionTrack`. Enable the `captions` feature to render with `Captions`.

Use plain-text VTT for phrase subtitles. Use JSON for explicitly timed words and metadata.
Never invent equal word durations from recorded speech: obtain real word timing or display
the whole phrase. Imported VTT markup, settings and inline timestamps remain source data;
the component does not interpret them as styling, placement or word timing.

```json
{
  "cues": [
    {
      "start": 0, "end": 2,
      "words": [
        {"text": "Choose", "start": 0, "end": 0.7},
        {"text": "Moon", "start": 0.9, "end": 1.5,
         "metadata": {"effect": "underline", "color": "#ffd166"}}
      ]
    }
  ]
}
```

Times are seconds on one clock. Word timestamps are absolute on that clock, not relative
to the cue. A cue has either `text` or `words`. Cues are ordered and may overlap for multiple
speakers; words are ordered, non-overlapping and contained by their cue. Optional `id`s
identify edits. Use `space_before: false` for attached punctuation.

**Boundary compatibility:** the new `Captions`, word states and `CaptionTrack::active_cues`
use `[start, end)`. Legacy `frame.get_subtitle_phrase` / `get_subtitle_cue` and the
`FFramesSubtitles` trait keep inclusive cue ends. Legacy `get_cue_stack` includes starts
strictly before `time + overlap`. Do not update existing snapshots to conceal a change
in those legacy behaviors. Zero-duration VTT cues remain visible only to the legacy
lookup at their exact timestamp; JSON cues still require a positive duration.

`Captions` defaults to scene-local time, matching frame animation. At video level the
scene and global clocks coincide. Use `.clock(CaptionClock::Global)` for a whole-video
transcript inside a scene. `render_at`, `render_at_with` and `highlight_at_with` accept
explicit track seconds when source timestamps need mapping; each also has a `try_` variant.

## Metadata is an application schema

Put reusable drawing code in Rust and choices/parameters in JSON. Prefer named enum
variants to numbers or contradictory flags. Keep defaults in Rust and only overrides in
JSON. Larger schemas can separate layout, motion and material settings.

```rust
#[derive(serde::Deserialize)]
#[serde(tag = "effect", rename_all = "kebab-case", deny_unknown_fields)]
enum Highlight {
    Underline { color: String },
    Fill { color: String },
}
```

No type registration is required. `metadata.typed::<T>()` lazily decodes the retained JSON
object the first time that type is requested. Each type has its own cached success or
failure, shared across frames, clones and threads. Another requested type cannot overwrite
or poison it. Unrequested types are never decoded. Missing/incompatible data returns `None`;
validate required metadata at construction and report the cue/word path when appropriate.

## Load once, compose locally

The same ordinary loaders work for every metadata type: `directory.process_media_source()`,
embedded `MyMedia::prepare()` and the normal browser bridge. Keep the component on the video.
Bounds are required in the constructor and use logical composition pixels, independent of
draft output scale.

```rust
use fframes::captions::{Captions, CaptionRect};

let directory = fframes::MediaDirectory::read_folder("media")?;
let media = directory.process_media_source()?;
let captions = Captions::from_media(
    "speech.json", CaptionRect::new(120.0, 700.0, 1680.0, 300.0))
    .font("DM Sans", 72);
// Store captions on the Video and pass media through RenderOptions.
```

A callback borrows the word scope and receives the preset SVG as a separate owned value:

```rust
self.captions.highlight_with(&mut frame, ctx, |scope, text| {
    match scope.word.metadata.typed::<Highlight>() {
        Some(Highlight::Underline { color }) => fframes::svgr!(
            <g>{text}
                <rect x="0" y={scope.height() - 4.0} width={scope.width()}
                    height="4" fill={color.as_str()} />
            </g>
        ),
        Some(Highlight::Fill { color }) => scope.mask_text(fframes::svgr!(
            <rect width={scope.width()} height={scope.height()} fill={color.as_str()} />
        )),
        None => text,
    }
})
```

`highlight_with` runs only for active timed words. `render_with` customizes every visible
word. Return `text` to preserve the preset; no clone is needed. Callback coordinates and
`scope.word.rect` are local to the word, starting at `(0, 0)`. FFrames applies placement;
`scope.composition_rect` names that placement explicitly. `words_at` returns positioned
advance rectangles for inspection. These are advance boxes, not tight glyph bounds.

`scope.mask_text(layer)` shows any SVG or shader through the font's glyph silhouette;
`scope.clip(layer)` clips to the local rectangle. Decorations can extend outside it.
Use `scope.id("name")` for SVG definitions. `time`, `elapsed()`, `word.progress` and `frame`
support animation. Build shaders once, cache glyph textures/geometry and change uniforms
per frame. Use Skia for shader rendering and inspect the actual pixels.

## Performance and verification

- Parse the document at media loading, once. JSON dispatch keeps its parsed result.
- Borrow `typed::<T>()` results during rendering. Never call JSON parsers or the uncached
  `metadata.parse::<T>()` helper inside a frame callback.
- Clones share cue data, raw JSON and typed caches. Editing metadata detaches that clone's
  cache; the next typed access reads the edited value automatically. Build a validated
  replacement `CaptionTrack` for changed cues.
- Keep `Captions` alive across frames. It caches layout by loaded track identity and retries
  missing fonts. Recreate it when font/layout settings change. Loading a replacement track
  under the same media name invalidates the layout.
- Inspect changed frames, including overlaps, gaps, exact boundaries and random seeks.
  For moving materials, check multiple points and readability over the real background.

Runtime JSON edits to existing effects need a data reload, not Rust compilation. New effect
code/schema, or changed embedded files, requires compilation. Data loading alone does not
provide a watcher or automatic player reload.
