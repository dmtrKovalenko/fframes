open CanvasSize
open Belt

let imageSize = 18
let imageData = Image.make(~width=imageSize, ~height=imageSize)
imageData->Image.setSrc(Icons.magnetRawIcon |> Js.String.concat("data:image/svg+xml;base64,"))

@ext
let renderMagent = (ctx, size, frame) => {
  let magnetX = frameToX(frame, size)

  ctx->Canvas2d.beginPath
  ctx->Canvas2d.moveTo(~x=magnetX, ~y=0.)
  ctx->Canvas2d.lineTo(~x=magnetX, ~y=size.height)
  ctx->Canvas2d.setStrokeStyle(String, "rgb(241 245 249 / 0.8)")
  ctx->Canvas2d.lineWidth(1.5)
  ctx->Canvas2d.stroke

  ctx->SceneMapCanvas.drawImage(
    ~imageData,
    ~dy=2,
    ~dx=magnetX->Int.fromFloat - imageSize - 5,
    ~dirtyWidth=imageSize,
    ~dirtyHeight=imageSize,
  )
}

@react.component
let make = (~size) => {
  let controlsCanvasRef = React.useRef(Js.Nullable.null)
  let editorContext = EditorContext.useEditorContext()
  let (player, _) = editorContext.usePlayer()

  useCanvasScale(controlsCanvasRef, size)

  React.useEffect1(() => {
    controlsCanvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(canvasElement => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(canvasElement)
      ctx->Canvas2d.clearRect(~x=0., ~y=0., ~w=size.scaledWidth, ~h=size.scaledHeight)

      Belt.Option.map(player.magnet, renderMagent(ctx, size))
    })
    ->ignore

    None
  }, [player.magnet])

  <canvas
    className="absolute inset-0"
    style={ReactDOMStyle.make(
      ~height=`${size.height->Float.toString}px`,
      ~width=`${size.width->Float.toString}px`,
      (),
    )}
    width={`${size.width->Js.Math.floor->Float.toString}px`}
    height={`${size.height->Js.Math.floor->Float.toString}px`}
    ref={ReactDOM.Ref.domRef(controlsCanvasRef)}
  />
}
