open Belt
open CanvasSize

module Canvas = Webapi.Canvas
module Canvas2d = Webapi.Canvas.Canvas2d

let renderSeekBar = (ctx, size, playState: Player.state) => {
  let x = frameToX(playState.frame, size)

  ctx->Canvas2d.beginPath
  ctx->Canvas2d.moveTo(~x, ~y=0.)
  ctx->Canvas2d.lineTo(~x, ~y=size.height)

  ctx->Canvas2d.moveTo(~x=x -. 2., ~y=0.)
  ctx->Canvas2d.lineTo(~x, ~y=7.)
  ctx->Canvas2d.lineTo(~x=x +. 2., ~y=0.)
  ctx->Canvas2d.lineTo(~x=x -. 2., ~y=0.)

  ctx->Canvas2d.setStrokeStyle(String, "#fbbf24")
  ctx->Canvas2d.stroke
  ctx->Canvas2d.closePath
}

let calculateFrameFromEvent = (e, ~size) => {
  let rectLeft =
    e
    ->ReactEvent.Mouse.target
    ->Web.Element.targetAsElement
    ->Webapi.Dom.Element.getBoundingClientRect
    ->Webapi.Dom.DomRect.left
    ->Int.fromFloat

  let x = e->ReactEvent.Mouse.clientX - rectLeft - timeline_margin_x / 2

  if x > 0 {
    (x->Float.fromInt *. size.pxToFrameRation)->Int.fromFloat
  } else {
    0
  }
}

@react.component
let make = (~size) => {
  let seekCanvasRef = React.useRef(Js.Nullable.null)
  let editorContext = EditorContext.useEditorContext()
  let (player, dispatch) = editorContext.usePlayer()

  useCanvasScale(seekCanvasRef, size)
  React.useEffect3(() => {
    seekCanvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(canvasElement => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(canvasElement)
      ctx->Canvas2d.clearRect(~x=0., ~y=0., ~w=size.scaledWidth, ~h=size.scaledHeight)

      switch player.playState {
      | CantPlay => ()
      | _ => renderSeekBar(ctx, size, player)
      }->ignore
    })
    ->ignore

    None
  }, (size, player.frame, player.playState))

  let hanldeMouseMove = Hooks.useEvent(e => {
    if player.playState !== Playing && Webapi.Dom.document->Web.Document.hasFocus {
      dispatch(NewFrame(calculateFrameFromEvent(e, ~size)))
    }
  })

  let handleClick = Hooks.useEvent(e => {
    let frame = calculateFrameFromEvent(e, ~size)

    dispatch(Seek(frame))
    dispatch(Play)
  })

  <canvas
    onClick=handleClick
    onMouseMove=hanldeMouseMove
    className={Cx.cx([
      "absolute inset-0",
      switch player.playState {
      | Paused | WaitingForAction => "cursor-col-resize"
      | Playing => "cursor-pointer"
      | CantPlay => "cursor-wait"
      },
    ])}
    style={ReactDOMStyle.make(
      ~height=`${size.height->Float.toString}px`,
      ~width=`${size.width->Float.toString}px`,
      (),
    )}
    width={`${size.width->Js.Math.floor->Float.toString}px`}
    height={`${size.height->Js.Math.floor->Float.toString}px`}
    ref={ReactDOM.Ref.domRef(seekCanvasRef)}
  />
}
