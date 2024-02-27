open Belt
open CanvasSize

module Canvas = Webapi.Canvas
module Canvas2d = Webapi.Canvas.Canvas2d

@send
external drawImage: (
  Canvas.Canvas2d.t,
  ~imageData: Image.t,
  ~dx: int,
  ~dy: int,
  ~dirtyWidth: int,
  ~dirtyHeight: int,
) => unit = "drawImage"

let renderRoundedRect = (ctx, ~x, ~y, ~width, ~height, ~radius, ()) => {
  ctx->Canvas2d.beginPath
  ctx->Canvas2d.moveTo(~x=x +. radius, ~y)

  ctx->Canvas2d.arcTo(~x1=x +. width, ~y1=y, ~x2=x +. width, ~y2=y +. height, ~r=radius)
  ctx->Canvas2d.arcTo(~x1=x +. width, ~y1=y +. height, ~x2=x, ~y2=y +. height, ~r=radius)
  ctx->Canvas2d.arcTo(~x1=x, ~y1=y +. height, ~x2=x, ~y2=y, ~r=radius)
  ctx->Canvas2d.arcTo(~x1=x, ~y1=y, ~x2=x +. width, ~y2=y, ~r=radius)

  ctx->Canvas2d.stroke
}

let clipOverTimeLineElement = (ctx, ~y, ~width, ~fill) => {
  let x = Float.fromInt(timeline_margin_x / 2)
  let height = Float.fromInt(scene_height_size)

  ctx->renderRoundedRect(~x, ~y, ~width, ~height, ~radius=12.0, ())
  ctx->Canvas2d.clip
  ctx->Canvas2d.setFillStyle(String, fill)
  ctx->Canvas2d.fillRect(~x, ~y, ~w=width, ~h=height)
}

let sceneColors = [
  "#f87171",
  "#fbbf24",
  "#4ade80",
  "#2dd4bf",
  "#38bdf8",
  "#818cf8",
  "#c084fc",
  "#f472b6",
  "#fb7185",
]

let renderScenes = (ctx, size: canvasSize, editorContext: EditorContext.editorContext) => {
  editorContext.videoMeta.scenesTimeline
  ->Js.Nullable.toOption
  ->Belt.Option.forEach(array =>
    array->Js.Array.forEachWithIndex((scene, i) => {
      let sceneColor = sceneColors |> Js.Array.length |> mod(i) |> Belt.Array.get(sceneColors)
      ctx->Canvas2d.setFillStyle(String, sceneColor->Utils.Option.unwrapOr("#fbbf24"))

      let x1 = scene.start->frameToX(size)
      let x2 = scene.end->frameToX(size)
      let overflowSafeX =
        array
        ->Js.Array.get(i - 1)
        ->Belt.Option.map(prev => frameToX(Utils.Math.maxI(prev.end, scene.start), size))
        ->Utils.Option.unwrapOr(x1)

      let width = x2 -. x1
      let markSize = 12

      // rescript cannot precalculate float expressions so using as much ints as possible
      let uninlided_y = timeline_scenes_start_y->Int.toFloat

      ctx->Canvas2d.globalAlpha(1.)

      ctx->Canvas2d.beginPath
      ctx->Canvas2d.moveTo(~x=x2, ~y=timeline_scenes_start_y->Float.fromInt)
      ctx->Canvas2d.lineTo(
        ~x=x2 -. markSize->Float.fromInt,
        ~y=timeline_scenes_start_y->Float.fromInt,
      )
      ctx->Canvas2d.lineTo(~x=x2, ~y=(timeline_scenes_start_y + markSize)->Float.fromInt)
      ctx->Canvas2d.fill
      ctx->Canvas2d.globalAlpha(0.8)
      ctx->Canvas2d.fillRect(~x=overflowSafeX, ~y=uninlided_y, ~w=x2 -. overflowSafeX, ~h=4.)
      ctx->Canvas2d.closePath

      ctx->Canvas2d.save

      ctx->Canvas2d.rect(~x=x1, ~y=uninlided_y, ~w=width -. 4., ~h=20.)
      ctx->Canvas2d.clip
      scene.name
      ->Js.String.split("::")
      ->Utils.Array.last
      ->Belt.Option.forEach(name =>
        name->Canvas2d.fillText(
          ctx,
          ~x=overflowSafeX +. 4.,
          ~y=(timeline_scenes_start_y + 16)->Int.toFloat,
        )
      )

      ctx->Canvas2d.restore

      ctx->Canvas2d.globalAlpha(0.25)
      ctx->Canvas2d.fillRect(~x=x1, ~y=uninlided_y, ~w=width, ~h=size.scaledHeight)
    })
  )

  ctx->Canvas2d.globalAlpha(1.)
}

let renderMainScene = (ctx, size, editorContext: EditorContext.editorContext) => {
  let aspectRatio =
    editorContext.videoMeta.width->Float.fromInt /. editorContext.videoMeta.height->Float.fromInt

  let width = (Float.fromInt(scene_height_size) *. aspectRatio)->Utils.Math.floor
  ctx->clipOverTimeLineElement(
    ~y=timeline_margin_y->Float.fromInt,
    ~width=size.maxSceneWidth,
    ~fill="#000",
  )

  let maxFramesInScene = size.maxSceneWidth->Float.toInt / width
  let framesBreak = editorContext.videoMeta.durationInFrames / maxFramesInScene

  Range.forEach(0, maxFramesInScene, i => {
    let svg = editorContext.wasmController.render_preview_frame(
      (i * framesBreak)->Js.BigInt.fromInt,
    )

    let image = Image.make(~width, ~height=scene_height_size)

    image->Image.setSrc(svg->Image.btoa |> Js.String.concat("data:image/svg+xml;base64,"))
    image->Image.onLoad(() => {
      ctx->Canvas2d.save
      ctx->drawImage(
        ~imageData=image,
        ~dy=timeline_margin_y,
        ~dx=timeline_margin_x / 2 + i * width,
        ~dirtyHeight=scene_height_size,
        ~dirtyWidth=width,
      )
      ctx->Canvas2d.restore
    })
  })

  ()
}

let renderAudioWaveForm = (
  ctx,
  ~endFrame,
  ~startFrame,
  ~x0,
  ~y0,
  ~audioSpaceWidth,
  ~audioName,
  ~editorContext: EditorContext.editorContext,
) => {
  let media = MediaLoader.MediaLoaderObserver.get().mediaList->Belt.Map.String.get(audioName)
  let audioInfo = switch media {
  | Some(Media(Audio(audioInfo))) => audioInfo
  | _ => failwith(`Unknown audio file ${audioName}. Did you forget to add it to your media folder?`)
  }

  let positionStart = 0
  let positionEnd = Js.Int.fromFloat(
    (endFrame - startFrame)->Float.fromInt /.
    editorContext.videoMeta.fps->Float.fromInt *.
    audioInfo.sampleRate->Float.fromInt,
  )
  let length = positionEnd - positionStart

  let step = 1.
  let sector = length->Js.Float.fromInt /. audioSpaceWidth *. step

  let position = ref(positionStart)
  let mid = Float.fromInt(audio_height / 2)
  let x = ref(x0)

  ctx->Canvas2d.beginPath
  ctx->Canvas2d.setStrokeStyle(String, "#e2e8f0")

  while x.contents < audioSpaceWidth || position.contents < positionEnd {
    let pcm = audioInfo.fltpData->Web.Float32Array.at(position.contents)->Utils.Option.unwrapOr(0.0)

    let y = mid +. y0 +. pcm *. mid
    ctx->Canvas2d.lineTo(~x=x.contents, ~y)

    position := (position.contents->Float.fromInt +. sector)->Utils.Math.floor
    x := (x.contents +. step)->Js.Math.floor
  }

  ctx->Canvas2d.stroke
}

let renderAudioMap = (ctx, size, editorContext: EditorContext.editorContext) => {
  let xStack = []

  editorContext.videoMeta.audioMap
  ->Js.Nullable.toOption
  ->Option.forEach(audioMap => audioMap->Js.Array.reduce((startY, track) => {
      let x = frameToX(track.start, size)

      let startY =
        xStack
        ->Array.getIndexBy(((lastX, _)) => x > lastX)
        ->Option.map(index => {
          let (_, startY) = xStack[index]->Utils.Option.unwrap
          xStack->Belt.Array.truncateToLengthUnsafe(index)

          startY
        })
        ->Utils.Option.unwrapOr(startY)

      let y = Float.fromInt(timeline_margin_y + scene_height_size + startY)
      let width = Float.fromInt(track.end - track.start) *. size.frameToPxRatio

      xStack->Js.Array.push((x +. width, startY))->ignore
      ctx->Canvas2d.save

      let textX = x +. 2.
      let textY = y -. 8.
      let textHeight = 14.

      // this is level 2 safe needed for a clip around track name text to prevent same stack names overflow.
      ctx->Canvas2d.save
      ctx->Canvas2d.rect(
        ~x=textX -. 10.,
        ~y=textY -. textHeight +. 4.,
        ~w=width +. 8.0,
        ~h=textHeight,
      )

      ctx->Canvas2d.clip
      ctx->Canvas2d.setFillStyle(String, "#e2e8f0")
      track.name->Canvas2d.fillText(ctx, ~x=textX, ~y=textY)
      ctx->Canvas2d.restore
      ctx->Canvas2d.beginPath

      ctx->renderRoundedRect(
        ~x,
        ~y,
        ~width,
        ~height=Float.fromInt(scene_height_size / 2),
        ~radius=8.0,
        (),
      )
      ctx->Canvas2d.clip

      ctx->Canvas2d.setFillStyle(String, "#059669")
      ctx->Canvas2d.fillRect(~x, ~y, ~w=width, ~h=Float.fromInt(scene_height_size / 2))

      ctx
      ->renderAudioWaveForm(
        ~x0=x,
        ~y0=y,
        ~audioName=track.name,
        ~audioSpaceWidth=width,
        ~editorContext,
        ~startFrame=track.start,
        ~endFrame=track.end,
      )
      ->ignore

      ctx->Canvas2d.closePath
      ctx->Canvas2d.restore

      startY + audio_height + audio_height / 2
    }, 32)->ignore)
}

let renderTimeSlots = (ctx, size, editorContext: EditorContext.editorContext) => {
  let coordinate_step = 100
  let full_timestamp_each_steps = 2

  let stepsCount =
    size.maxSceneWidth
    ->Utils.Math.divideFloat(coordinate_step->Float.fromInt)
    ->Js.Math.floor
    ->Float.toInt

  let stepDuration = editorContext.videoMeta.durationInFrames / stepsCount

  Range.forEach(0, stepsCount, i => {
    let x = (i * coordinate_step + timeline_margin_x / 2)->Float.fromInt

    ctx->Canvas2d.beginPath
    ctx->Canvas2d.moveTo(~x, ~y=0.)
    ctx->Canvas2d.lineTo(~x, ~y=18.)

    ctx->Canvas2d.setStrokeStyle(String, "#475569")
    ctx->Canvas2d.stroke

    if mod(i, full_timestamp_each_steps) === 0 {
      ctx->Canvas2d.font("12px sans-serif")
      ctx->Canvas2d.setFillStyle(String, "#64748b")

      (i * stepDuration)
      ->Float.fromInt
      ->Utils.Math.divideFloat(editorContext.videoMeta.fps->Float.fromInt)
      ->Utils.Duration.formatSeconds
      ->Canvas2d.fillText(ctx, ~x=x +. 8., ~y=14.)
    }
  })
}

@react.component
let make = (~size: canvasSize) => {
  let canvasRef = React.useRef(Js.Nullable.null)
  let editorContext = EditorContext.useEditorContext()

  useCanvasScale(canvasRef, size)

  React.useEffect1(() => {
    canvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(element => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(element)

      ctx->renderTimeSlots(size, editorContext)
      ctx->renderScenes(size, editorContext)

      ctx->Canvas2d.save
      ctx->renderAudioMap(size, editorContext)
      ctx->Canvas2d.restore
      ctx->renderMainScene(size, editorContext)

      ()
    })
    ->ignore

    None
  }, [size])

  <canvas
    className="absolute inset-0"
    style={ReactDOMStyle.make(
      ~height=`${size.height->Float.toString}px`,
      ~width=`${size.width->Float.toString}px`,
      (),
    )}
    width={`${size.scaledWidth->Js.Math.floor->Float.toString}px`}
    height={`${size.scaledHeight->Js.Math.floor->Float.toString}px`}
    ref={ReactDOM.Ref.domRef(canvasRef)}
  />
}
