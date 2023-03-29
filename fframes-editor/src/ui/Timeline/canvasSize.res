type canvasSize = {
  width: float,
  height: float,
  scale: float,
  scaledWidth: float,
  scaledHeight: float,
  maxSceneWidth: float,
  frameToPxRatio: float,
  pxToFrameRation: float,
}

// Make sure to not change this from ints to float to enable preval of calculations
@inline
let timeline_margin_x = 64
@inline
let timeline_margin_y = 64
@inline
let scene_height_size = 120
@inline
let timeline_scenes_start_y = 24

let audio_height = scene_height_size / 2

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

let frameToX = (frame, size: canvasSize) =>
  Belt.Float.fromInt(frame) *. size.frameToPxRatio +. (timeline_margin_x / 2)->Belt.Float.fromInt
