open CanvasSize
open Belt

let imageSize = 18

// The system "Pin" icon marks the magnet (restart) point
let pinPath = "M12.8636 3.26026C13.9444 1.74705 16.1254 1.56655 17.4403 2.88148L21.1185 6.55971C22.4335 7.87464 22.2529 10.0556 20.7397 11.1364L16.4786 14.1801C16.1638 14.405 16 14.7305 16 15V17.5C16 18.9069 15.0409 19.9513 13.976 20.4104C12.9046 20.8724 11.4792 20.8468 10.4568 19.8243L8.02332 17.3909L3.70711 21.7071C3.31658 22.0976 2.68342 22.0976 2.29289 21.7071C1.90237 21.3166 1.90237 20.6834 2.29289 20.2929L6.60911 15.9767L4.17567 13.5432C3.1532 12.5208 3.12762 11.0954 3.58957 10.024C4.04871 8.95908 5.09306 8 6.5 8H9C9.26948 8 9.59505 7.8362 9.81994 7.52136L12.8636 3.26026ZM8.73001 15.2692L11.871 18.4101C12.1769 18.716 12.6696 18.7957 13.1842 18.5739C13.7052 18.3492 14 17.9208 14 17.5V15C14 13.9717 14.5749 13.0821 15.3162 12.5526L19.5773 9.50895C20.0848 9.14643 20.1453 8.41495 19.7043 7.97392L16.0261 4.29569C15.5851 3.85467 14.8536 3.9152 14.491 4.42273L11.4474 8.68383C10.9179 9.42507 10.0283 10 9 10H6.5C6.07925 10 5.65079 10.2948 5.42615 10.8158C5.20431 11.3304 5.28397 11.8231 5.58988 12.129L8.73001 15.2692Z"

let makePinImage = color => {
  let image = Image.make(~width=imageSize, ~height=imageSize)
  let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path fill="${color}" d="${pinPath}"/></svg>`
  image->Image.setSrc(svg->Image.btoa |> Js.String.concat("data:image/svg+xml;base64,"))
  image
}

let darkPin = makePinImage(Theme.dark.text)
let lightPin = makePinImage(Theme.light.text)

let pinImage = theme =>
  switch theme {
  | Theme.Dark => darkPin
  | Theme.Light => lightPin
  }

@ext
let rendermagnet = (ctx, size, frame, theme) => {
  let palette = Theme.palette(theme)
  let imageData = pinImage(theme)
  let magnetX = frameToX(frame, size)
  let timelineStart = size.timelineMarginLeft->Float.fromInt
  let timelineEnd = timelineStart +. size.maxSceneWidth

  // Check if magnet is within visible timeline area
  // When zoomed out, magnet should be visible if it's within the timeline area
  let isMagnetVisible = magnetX >= timelineStart && magnetX <= timelineEnd

  if isMagnetVisible {
    // Render normal magnet line and icon
    ctx->Canvas2d.beginPath
    ctx->Canvas2d.moveTo(~x=magnetX, ~y=0.)
    ctx->Canvas2d.lineTo(~x=magnetX, ~y=size.height)
    ctx->Canvas2d.setStrokeStyle(String, palette.textSecondary)
    ctx->Canvas2d.lineWidth(1.)
    ctx->Canvas2d.stroke

    ctx->SceneMapCanvas.drawImage(
      ~imageData,
      ~dy=2,
      ~dx=magnetX->Int.fromFloat - imageSize - 5,
      ~dirtyWidth=imageSize,
      ~dirtyHeight=imageSize,
    )
  } else {
    // Render corner indicator when magnet is outside viewport
    let cornerX = if magnetX < timelineStart {
      // Magnet is to the left - show on left corner
      timelineStart +. 10.0
    } else {
      // Magnet is to the right - show on right corner
      timelineEnd -. 30.0
    }

    // Draw corner magnet icon with different styling
    ctx->Canvas2d.save
    ctx->Canvas2d.globalAlpha(0.7)
    ctx->SceneMapCanvas.drawImage(
      ~imageData,
      ~dy=2,
      ~dx=cornerX->Int.fromFloat,
      ~dirtyWidth=imageSize,
      ~dirtyHeight=imageSize,
    )
    ctx->Canvas2d.restore
  }
}

@react.component
let make = (~size) => {
  let controlsCanvasRef = React.useRef(Js.Nullable.null)
  let editorContext = EditorContext.useEditorContext()
  let (player, _) = editorContext.usePlayer()
  let theme = Theme.use()

  useCanvasScale(controlsCanvasRef, size)

  React.useEffect3(() => {
    controlsCanvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(canvasElement => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(canvasElement)
      ctx->Canvas2d.clearRect(~x=0., ~y=0., ~w=size.scaledWidth, ~h=size.scaledHeight)

      Belt.Option.map(player.magnet, frame => rendermagnet(ctx, size, frame, theme))
    })
    ->ignore

    None
  }, (player.magnet, player.viewportOffset, theme))

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
