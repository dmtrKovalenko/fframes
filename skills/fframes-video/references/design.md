# Designing good-looking fframes videos

Generated videos usually go wrong the same ways: too much text, everything moving at once,
linear motion, no hierarchy, cramped margins and random colors. A simple design without those
problems already looks finished.

## Matching an existing video

- Probe the reference's dimensions, rational frame rate and frame count. Map cuts and short
  transitions in source frames before writing replacement scenes; duration alone does not
  establish a matching edit. Apply the pacing defaults below when designing original work.
- Extract consecutive frames around fast effects and cut boundaries. Sparse contact sheets
  can miss a sub-second reveal or conceal independent object motion. Compare reference and
  output at the same source frames, including full-size text and logo details.
- Identify which motion belongs to the camera and which belongs to individual objects.
  A single transformed wall cannot reproduce cards separating at different depths. Use
  separate object poses and perspective projection for that effect; SVG skew is a 2D transform.
- Inspect supplied shader sources and existing project assets before approximating them.
  Matching an effect also requires its scene parameters, masks, lighting, colors and timing.
  Reuse existing vector wordmarks and logo animations when requested; enlarging a small
  raster logo or glyph atlas will not preserve their sharpness.
- When inserting a UI recording, review the selected range itself: stable window bounds,
  visible controls, and the requested interaction throughout. A recording made during window
  resizing may need a later crop. Keep playback speed consistent with the intended gesture.

## Pacing

- Reading time: about 3 words per second plus 1 s to notice the text. A 9 word line needs
  ~4 s on screen after it finished animating in.
- Scene lengths: title 2.5-4 s, content 4-8 s, outro 2-3 s. Social clips: first frame must
  already show something (no fade from black) and the hook lands in the first 1.5 s.
- Entrances take 0.3-0.6 s, exits 0.2-0.3 s. Holds (nothing moves) of 1-3 s are good; keep a
  little motion somewhere so they do not look frozen.
- Sync cuts and entrances to the beat or the voice (see audio.md).

## Layout

- Safe margins: 8-10% of the width on each side for landscape; portrait keeps content inside
  x 8-92%, y 12-78% (platform UI covers the top and bottom).
- Align to a grid: pick 2-3 x positions (left edge, center, 60%) and reuse them. Left-aligned
  text blocks read better than centered paragraphs; center only short titles.
- Whitespace is the most effective "design": one element per scene is fine.
- Vertical rhythm: line height 1.2-1.35 x font size; gaps between groups 2x the line gap.

## Typography

- One family (two at most: a display face for titles, a text face for body). Ship the files
  in `media/` and check they contain every glyph the text uses.
- Sizes at 1920x1080: hero 140-200, title 96-120, subtitle 44-56, body 44-60, caption 28-34.
  Never below 28 px. Portrait at 1080 wide uses about the same pixel sizes.
- Weights: title 600-800, body 400-500. Letter-spacing: `-1` to `-3` for big titles,
  `+2` to `+6` for small caps labels.
- Max ~8 words per line, ~3 lines per card. Measure with `frame.text_width` or wrap with
  `frame.text_break_lines`; shrink to fit with a loop over sizes (as in the
  [motion-graphics quote card](https://github.com/dmtrKovalenko/fframes/tree/main/examples/motion-graphics/src/quote_card.rs)) and memoize the result
  in a `OnceLock`. Generated projects include a `FittedSizes` helper that does this.

## Color

- A background, a foreground (text), one accent, one or two supporting tints.
- Contrast: body text at least 4.5:1 against its background. Check suspicious frames at full
  size.
- Blur and drop shadow filters are expensive on the CPU backend; keep them on static subtrees (no `{}` inside) so they are
  cached, or use the Skia backend.

## Motion

Use **`frame.animate(fframes::timeline!(...))` directly in `svgr!` attributes** as the default
for motion with fixed values. `svgr!` lifts these inline timelines into cached statics, so
their easing runtimes are initialized once, not rebuilt every frame. Write easings such as
`Easing::Spring { ... }` or `Easing::CubicBezier(...)` directly inside the `timeline!` block.
Named easing constants are optional when reusing a preset.

Easing presets that look good (`use fframes::animation::Easing`); the values can be inlined:

```rust
const SPRING_SNAPPY: Easing = Easing::Spring { mass: 1.0, stiffness: 300.0, damping: 26.0 }; // UI-like, tiny overshoot
const SPRING_SOFT: Easing = Easing::Spring { mass: 1.0, stiffness: 150.0, damping: 18.0 };   // noticeable, friendly
const SPRING_BOUNCY: Easing = Easing::Spring { mass: 1.0, stiffness: 220.0, damping: 12.0 }; // playful, use rarely
const EASE_OUT_EXPO: Easing = Easing::CubicBezier(0.16, 1.0, 0.3, 1.0);  // fast then settles, great for slides
const EASE_IN_OUT: Easing = Easing::CubicBezier(0.65, 0.0, 0.35, 1.0);  // camera moves, morphs
```

Not set in stone rules, but a good start for a dynamic animation:
- Enter with ease-out or a spring from a short distance (40-120 px) plus opacity 0 -> 1.
  Exit with ease-in, faster, to a shorter distance or just opacity.
- Stagger related items by 60-120 ms (lists, words, letters by 20-40 ms).
- Animate transform and opacity; animating font size or layout causes jitter.
- Scale from 0.9-0.96, not from 0. Rotation within ±6°.
- One focal movement at a time; secondary elements move less and later.
- Loops (`frame.animate_loop`) for ambient motion: long (6-12 s), small amplitude.

### Recipes

Staggered entrance with timelines and easings inline in `svgr!`. The subtitle follows the
title by 90 ms; `svgr!` caches all four animations:

```rust
use fframes::{Transform, animation::Easing};

fframes::svgr!(
    <g>
        <g transform={frame.animate(fframes::timeline!(
            at 0.4, animate Transform::translate(0, 60) => Transform::translate(0, 0),
                Easing::Spring { mass: 1.0, stiffness: 150.0, damping: 18.0 },
        ))}
        opacity={frame.animate(fframes::timeline!(
            at 0.4 => 0.7, animate 0.0 => 1.0, Easing::EaseOut,
        ))}>
            <text x="160" y="420" font-family="DM Sans" font-size="120" fill="#fff">
                "Shader mode"
            </text>
        </g>
        <g transform={frame.animate(fframes::timeline!(
            at 0.49, animate Transform::translate(0, 40) => Transform::translate(0, 0),
                Easing::Spring { mass: 1.0, stiffness: 150.0, damping: 18.0 },
        ))}
        opacity={frame.animate(fframes::timeline!(
            at 0.49 => 0.79, animate 0.0 => 1.0, Easing::EaseOut,
        ))}>
            <text x="160" y="510" font-family="DM Sans" font-size="48" fill="#fff">
                "Made with fframes"
            </text>
        </g>
    </g>
)
```

Leave the spring's end time unspecified so it can settle naturally. Keep the attribute
expression as a direct `frame.animate(timeline!(...))` call so the optimizer recognizes it.
For starts or values that depend on item data, or animations outside this pattern (including
`animate_loop`), construct timelines once on `self`. Use cached `AnimationRuntime` values
for starts that change after construction or custom/subframe clocks. See `api.md` for the
optimizer's supported forms and timing boundaries.

Some examples for inspiration:

Word-by-word title reveal: split into words, lay them out with `frame.text_width` per word
(or `text-anchor="start"` with measured offsets), stagger each word 40-60 ms with a small
upward spring.

Counter ("0 -> 12,480"): animate an `f32` with `EASE_OUT_EXPO` over 1.2-1.8 s and format with
thousands separators; use a font with tabular figures (or a monospace font) so digits do not
jiggle, and right-align it with `text-anchor="end"`.

Growing bar (charts, highlights): animate a `<rect>` width from a hairline (`0.5`, never `0`, SVG
rejects zero sized rects) to its target with `EASE_IN_OUT`.

Underline or stroke draw-on: `stroke-dasharray={len}` and animate `stroke-dashoffset` from
`len` to `0`.

Scene transitions: `fn overlap(&self) -> Overlap { Overlap::Previous(0.4) }` plus both
scenes fading (outgoing 1 -> 0, incoming 0 -> 1) is a clean cross-fade; a "push" moves the
outgoing scene -80 px while the incoming comes from +80 px.

Lower third: a bar slides in from the left (`EASE_OUT_EXPO`, 0.5 s), the name fades up 0.15 s
later, the role 0.1 s after that, holds, then everything exits together in 0.25 s.

## Review checklist

Run `strip` for every scene and `frame` for the key moments, then check:

- [ ] Nothing important within the outer 8% (or the portrait UI areas).
- [ ] Text fits its box and the canvas; no line longer than ~8 words; nothing below 28 px.
- [ ] Every text is on screen long enough to read (3 words/s + 1 s after it settles).
- [ ] Only one thing draws the eye at a time; entrances are staggered, not simultaneous.
- [ ] Consistent margins, alignment and colors across scenes.
- [ ] Contrast is sufficient on every background (including gradients behind text).
- [ ] Transitions have no empty or flashing frame (check `Scene@end` and the next scene's
      first frame, and the `onion` of the transition).
- [ ] The first frame is not blank for social formats.
- [ ] `inspect` is clean; audio levels fit (see audio.md).
