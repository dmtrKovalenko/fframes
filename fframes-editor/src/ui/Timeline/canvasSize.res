type canvasSize = {
  width: float,
  height: float,
  scale: float,
  scaledWidth: float,
  scaledHeight: float,
  maxSceneWidth: float,
  frameToPxRatio: float,
  pxToFrameRatio: float,
  viewportOffset: float,
  timelineMarginLeft: int,
  timelineMarginRight: int,
  // Top of the filmstrip: under the scene strip when the video has scenes, else right under the ruler
  lanesTop: int,
}

// Make sure to not change this from ints to float to enable preval of calculations
@inline
let timeline_margin_x = 64
@inline
let scene_height_size = 120
// The ruler: tick marks and timestamps, closed by a hairline
@inline
let ruler_height = 24
@inline
let timeline_scenes_start_y = 28
// Filmstrip top with and without a scene strip (ruler, gap, scene bar and its name, gap)
@inline
let lanes_top_with_scenes = 60
@inline
let lanes_top_without_scenes = 32
// Gap between the filmstrip and the first audio clip, and between stacked clips
@inline
let audio_gap = 8
// The clip name sits in this band, the waveform below it
@inline
let audio_label_height = 20

let audio_height = scene_height_size / 2

let lanesTop = (scenesTimeline: Js.Nullable.t<array<'a>>) =>
  switch scenesTimeline->Js.Nullable.toOption {
  | Some(scenes) if Js.Array.length(scenes) > 0 => lanes_top_with_scenes
  | _ => lanes_top_without_scenes
  }

module Canvas = Webapi.Canvas
module Canvas2d = Webapi.Canvas.Canvas2d

let useCanvasScale = (elementRef: React.ref<'a>, size) => {
  React.useEffect1(() => {
    elementRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.forEach(canvasElement => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(canvasElement)

      canvasElement->Canvas.CanvasElement.setHeight(
        size.scaledHeight->Js.Math.floor->Belt.Float.toInt,
      )
      canvasElement->Canvas.CanvasElement.setWidth(
        size.scaledWidth->Js.Math.floor->Belt.Float.toInt,
      )

      ctx->Canvas2d.scale(~x=size.scale, ~y=size.scale)
    })

    None
  }, [size])
}

let calculateTimelineMargins = (
  viewportOffset: float,
  ~videoTotalWidth: float,
  ~visibleWidth: float,
) => {
  // Calculate if we're at the video start or end
  let atVideoStart = viewportOffset <= 0.0
  let atVideoEnd = viewportOffset +. visibleWidth >= videoTotalWidth

  // Calculate left margin when at video start
  let leftMargin = if atVideoStart {
    // At video start: show margin proportional to how much video start is visible
    let visibleStartPortion = Js.Math.min(32.0, -.viewportOffset +. 32.0)
    Js.Math.max(0.0, visibleStartPortion)->Belt.Float.toInt
  } else {
    0
  }

  // Calculate right margin when at video end
  let rightMargin = if atVideoEnd {
    // At video end: show margin proportional to how much past the end we can see
    let pastEndPortion = viewportOffset +. visibleWidth -. videoTotalWidth
    let visibleEndPortion = Js.Math.min(32.0, pastEndPortion +. 32.0)
    Js.Math.max(0.0, visibleEndPortion)->Belt.Float.toInt
  } else {
    0
  }

  (leftMargin, rightMargin)
}

// Backward compatibility function for left margin only
let calculateTimelineMargin = (
  viewportOffset: float,
  ~videoTotalWidth: float,
  ~visibleWidth: float,
) => {
  let (leftMargin, _) = calculateTimelineMargins(viewportOffset, ~videoTotalWidth, ~visibleWidth)
  leftMargin
}

let frameToX = (frame, size: canvasSize) =>
  Belt.Float.fromInt(frame) *. size.frameToPxRatio +.
  Belt.Float.fromInt(size.timelineMarginLeft) -.
  size.viewportOffset
