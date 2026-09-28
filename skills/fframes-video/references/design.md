# Designing good-looking fframes videos

Generated videos usually go wrong the same ways: too much text, everything moving at once,
linear motion, no hierarchy, cramped margins and random colors. A simple design without those
problems already looks finished.

## Pacing

- Reading time: about 3 words per second plus 1 s to notice the text. A 9 word line needs
  ~4 s on screen after it finished animating in.
- Scene lengths: title 2.5-4 s, content 4-8 s, outro 2-3 s. Social clips: first frame must
  already show something (no fade from black) and the hook lands in the first 1.5 s.
- Entrances take 0.3-0.6 s, exits 0.2-0.3 s. Holds (nothing moves) of 1-3 s are good; keep a
  slow background drift so they do not look frozen.
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
  in `media/`. Good free choices: Inter, DM Sans, Manrope, Space Grotesk (UI/tech), Playfair
  Display / Fraunces (editorial), JetBrains Mono (code), Bebas Neue (condensed display).
- Sizes at 1920x1080: hero 140-200, title 96-120, subtitle 44-56, body 44-60, caption 28-34.
  Never below 28 px. Portrait at 1080 wide uses about the same pixel sizes.
- Weights: title 600-800, body 400-500. Letter-spacing: `-1` to `-3` for big titles,
  `+2` to `+6` for small caps labels.
- Max ~8 words per line, ~3 lines per card. Measure with `frame.text_width` or wrap with
  `frame.text_break_lines`; shrink to fit with a loop over sizes (as in the
  [motion-graphics quote card](https://github.com/dmtrKovalenko/fframes/tree/main/examples/motion-graphics/src/quote_card.rs)) and memoize the result
  in a `OnceLock`. Generated projects include a `FittedSizes` helper that does this.

## Color

- A background, a foreground (text), one accent, one or two supporting tints. Example sets:
  - Light: bg `#f8fafc`, text `#0f172a`, muted `#475569`, accent `#6366f1`, tint `#c7d2fe`.
  - Dark: bg `#0b1020`, text `#e2e8f0`, muted `#94a3b8`, accent `#22d3ee`, tint `#1e293b`.
  - Warm: bg `#fff7ed`, text `#1c1917`, accent `#ea580c`, tint `#fed7aa`.
- Contrast: body text at least 4.5:1 against its background. Check suspicious frames at full
  size.
- Depth without effects: a large soft radial gradient behind content, slightly different
  surface color for cards, a 1-2 px border of a lighter tint. Blur and drop shadow filters are
  expensive on the CPU backend; keep them on static subtrees (no `{}` inside) so they are
  cached, or use the Skia backend.

## Motion

Easing presets that look good (`use fframes::animation::Easing`):

```rust
const SPRING_SNAPPY: Easing = Easing::Spring { mass: 1.0, stiffness: 300.0, damping: 26.0 }; // UI-like, tiny overshoot
const SPRING_SOFT: Easing = Easing::Spring { mass: 1.0, stiffness: 150.0, damping: 18.0 };   // noticeable, friendly
const SPRING_BOUNCY: Easing = Easing::Spring { mass: 1.0, stiffness: 220.0, damping: 12.0 }; // playful, use rarely
const EASE_OUT_EXPO: Easing = Easing::CubicBezier(0.16, 1.0, 0.3, 1.0);  // fast then settles, great for slides
const EASE_IN_OUT: Easing = Easing::CubicBezier(0.65, 0.0, 0.35, 1.0);  // camera moves, morphs
```

Principles:
- Enter with ease-out or a spring from a short distance (40-120 px) plus opacity 0 -> 1.
  Exit with ease-in, faster, to a shorter distance or just opacity.
- Stagger related items by 60-120 ms (lists, words, letters by 20-40 ms).
- Animate transform and opacity; animating font size or layout causes jitter.
- Scale from 0.9-0.96, not from 0. Rotation within ±6°.
- One focal movement at a time; secondary elements move less and later.
- Loops (`frame.animate_loop`) for ambient motion: long (6-12 s), small amplitude.

### Recipes

Staggered entrance with a runtime start time:

```rust
fn enter(frame: &Frame, start: f32) -> (f32, f32) {
    let spring = fframes::animation::AnimationRuntime::new(3.0, &SPRING_SOFT);
    let fade = fframes::animation::AnimationRuntime::new(0.3, &Easing::EaseOut);
    let y = frame.animate_runtime(fframes::AnimateRuntimeInput { on_second: start, from: 60.0_f32, to: 0.0, animation_runtime: &spring });
    let opacity = frame.animate_runtime(fframes::AnimateRuntimeInput { on_second: start, from: 0.0_f32, to: 1.0, animation_runtime: &fade });
    (y, opacity)
}
// items.iter().enumerate().map(|(i, item)| { let (y, o) = enter(&frame, 0.4 + i as f32 * 0.09); svgr!(<g opacity={o} transform={Transform::translate(0, y)}>...</g>) })
```

Word-by-word title reveal: split into words, lay them out with `frame.text_width` per word
(or `text-anchor="start"` with measured offsets), stagger each word 40-60 ms with a small
upward spring.

Counter ("0 -> 12,480"): animate an `f32` with `EASE_OUT_EXPO` over 1.2-1.8 s and format with
thousands separators; use a font with tabular figures (or a monospace font) so digits do not
jiggle, and right-align it with `text-anchor="end"`.

Progress / highlight bar: animate a `<rect>` width from a hairline (`0.5`, never `0`, SVG
rejects zero sized rects) to its target with `EASE_IN_OUT`.

Underline or stroke draw-on: `stroke-dasharray={len}` and animate `stroke-dashoffset` from
`len` to `0`.

Scene transitions: `fn overlap(&self) -> Overlap { Overlap::Previous(0.4) }` plus both
scenes fading (outgoing 1 -> 0, incoming 0 -> 1) is a clean cross-fade; a "push" moves the
outgoing scene -80 px while the incoming comes from +80 px.

Lower third: a bar slides in from the left (`EASE_OUT_EXPO`, 0.5 s), the name fades up 0.15 s
later, the role 0.1 s after that, holds, then everything exits together in 0.25 s.

Background life: two large blurred radial gradients (static markup inside a `<g>`) whose
wrapping transforms drift 50-150 px over 8-12 s with `animate_loop`.

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
