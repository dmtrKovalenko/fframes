open Belt
open CanvasSize
open Webapi

module Canvas = Webapi.Canvas
module Canvas2d = Webapi.Canvas.Canvas2d
module DocumentEvent = Dom.EventTarget.Impl(Dom.Window)

let previewImageCache = ref(Belt.Map.String.empty)
let maxCacheSize = 500

// Clean up cache when it gets too large
let cleanupCache = () => {
  let currentSize = previewImageCache.contents->Belt.Map.String.size
  if currentSize > maxCacheSize {
    // Keep only the most recent half of the cache
    let keysArray = previewImageCache.contents->Belt.Map.String.keysToArray
    let keepSize = maxCacheSize / 2
    let keysToRemove = Belt.Array.slice(keysArray, ~offset=0, ~len=currentSize - keepSize)

    previewImageCache :=
      Belt.Array.reduce(keysToRemove, previewImageCache.contents, (cache, key) =>
        cache->Belt.Map.String.remove(key)
      )
  }
}

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

let fillRoundedRect = (ctx, ~x, ~y, ~width, ~height, ~radius, ()) => {
  ctx->Canvas2d.beginPath
  ctx->Canvas2d.moveTo(~x=x +. radius, ~y)

  ctx->Canvas2d.arcTo(~x1=x +. width, ~y1=y, ~x2=x +. width, ~y2=y +. height, ~r=radius)
  ctx->Canvas2d.arcTo(~x1=x +. width, ~y1=y +. height, ~x2=x, ~y2=y +. height, ~r=radius)
  ctx->Canvas2d.arcTo(~x1=x, ~y1=y +. height, ~x2=x, ~y2=y, ~r=radius)
  ctx->Canvas2d.arcTo(~x1=x, ~y1=y, ~x2=x +. width, ~y2=y, ~r=radius)

  ctx->Canvas2d.fill
}

let renderRoundedCorners = (
  ctx,
  ~x,
  ~y,
  ~width,
  ~height,
  ~topLeft=0.0,
  ~topRight=0.0,
  ~bottomLeft=0.0,
  ~bottomRight=0.0,
  (),
) => {
  ctx->Canvas2d.beginPath
  ctx->Canvas2d.moveTo(~x=x +. topLeft, ~y)

  // Top edge
  ctx->Canvas2d.lineTo(~x=x +. width -. topRight, ~y)
  if topRight > 0.0 {
    ctx->Canvas2d.arcTo(~x1=x +. width, ~y1=y, ~x2=x +. width, ~y2=y +. topRight, ~r=topRight)
  }

  // Right edge
  ctx->Canvas2d.lineTo(~x=x +. width, ~y=y +. height -. bottomRight)
  if bottomRight > 0.0 {
    ctx->Canvas2d.arcTo(
      ~x1=x +. width,
      ~y1=y +. height,
      ~x2=x +. width -. bottomRight,
      ~y2=y +. height,
      ~r=bottomRight,
    )
  }

  // Bottom edge
  ctx->Canvas2d.lineTo(~x=x +. bottomLeft, ~y=y +. height)
  if bottomLeft > 0.0 {
    ctx->Canvas2d.arcTo(~x1=x, ~y1=y +. height, ~x2=x, ~y2=y +. height -. bottomLeft, ~r=bottomLeft)
  }

  // Left edge
  ctx->Canvas2d.lineTo(~x, ~y=y +. topLeft)
  if topLeft > 0.0 {
    ctx->Canvas2d.arcTo(~x1=x, ~y1=y, ~x2=x +. topLeft, ~y2=y, ~r=topLeft)
  }

  ctx->Canvas2d.closePath
}

let clipOverTimeLineElement = (ctx, size: canvasSize, ~y, ~width, ~fill) => {
  let x = Float.fromInt(size.timelineMarginLeft)
  let height = Float.fromInt(scene_height_size)

  ctx->renderRoundedRect(~x, ~y, ~width, ~height, ~radius=12.0, ())
  ctx->Canvas2d.clip
  ctx->Canvas2d.setFillStyle(String, fill)
  ctx->Canvas2d.fillRect(~x, ~y, ~w=width, ~h=height)
}

let renderScenes = (
  ctx,
  size: canvasSize,
  editorContext: EditorContext.editorContext,
  ~palette: Theme.palette,
) => {
  let sceneColors = palette.scenes

  editorContext.videoMeta.scenesTimeline
  ->Js.Nullable.toOption
  ->Belt.Option.forEach(array =>
    array->Js.Array.forEachWithIndex((scene, i) => {
      // Skip scenes that start beyond video duration
      if scene.start < editorContext.videoMeta.durationInFrames {
        let sceneColor = sceneColors |> Js.Array.length |> mod(i) |> Belt.Array.get(sceneColors)
        let sceneColor = sceneColor->Utils.Option.unwrapOr(palette.accent)
        ctx->Canvas2d.setFillStyle(String, sceneColor)

        // Clamp scene boundaries to video duration to prevent rendering beyond video end
        let clampedStart = Utils.Math.minI(scene.start, editorContext.videoMeta.durationInFrames)
        let clampedEnd = Utils.Math.minI(scene.end, editorContext.videoMeta.durationInFrames)

        let x1 = clampedStart->frameToX(size)
        let x2 = clampedEnd->frameToX(size)
        let overflowSafeX =
          array
          ->Js.Array.get(i - 1)
          ->Belt.Option.map(prev =>
            frameToX(
              Utils.Math.maxI(
                Utils.Math.minI(prev.end, editorContext.videoMeta.durationInFrames),
                clampedStart,
              ),
              size,
            )
          )
          ->Utils.Option.unwrapOr(x1)

        // Constrain scene end position to respect right margin
        let timelineEnd = size.timelineMarginLeft->Float.fromInt +. size.maxSceneWidth
        let constrainedX2 = Js.Math.min(x2, timelineEnd)
        let width = constrainedX2 -. x1
        let markSize = 12

        // rescript cannot precalculate float expressions so using as much ints as possible
        let uninlided_y = timeline_scenes_start_y->Int.toFloat

        ctx->Canvas2d.globalAlpha(1.)

        ctx->Canvas2d.beginPath
        ctx->Canvas2d.moveTo(~x=constrainedX2, ~y=timeline_scenes_start_y->Float.fromInt)
        ctx->Canvas2d.lineTo(
          ~x=constrainedX2 -. markSize->Float.fromInt,
          ~y=timeline_scenes_start_y->Float.fromInt,
        )
        ctx->Canvas2d.lineTo(
          ~x=constrainedX2,
          ~y=(timeline_scenes_start_y + markSize)->Float.fromInt,
        )
        ctx->Canvas2d.fill
        ctx->Canvas2d.fillRect(
          ~x=overflowSafeX,
          ~y=uninlided_y,
          ~w=constrainedX2 -. overflowSafeX,
          ~h=4.,
        )
        ctx->Canvas2d.closePath

        ctx->Canvas2d.save

        ctx->Canvas2d.rect(~x=x1, ~y=uninlided_y, ~w=width -. 4., ~h=20.)
        ctx->Canvas2d.clip
        ctx->Canvas2d.font(Theme.canvasFontMedium)
        ctx->Canvas2d.setFillStyle(String, palette.textSecondary)
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

        ctx->Canvas2d.setFillStyle(String, sceneColor)
        ctx->Canvas2d.globalAlpha(0.05)
        ctx->Canvas2d.fillRect(~x=x1, ~y=uninlided_y, ~w=width, ~h=size.scaledHeight)
      }
    })
  )

  ctx->Canvas2d.globalAlpha(1.)
}

// Unified function to render a frame with proper clipping and rounded corners
let renderFrameWithClipping = (
  ctx,
  ~image,
  ~previewX,
  ~renderWidth,
  ~frameNumber,
  ~i,
  ~videoEndX,
  ~endX,
  ~editorContext: EditorContext.editorContext,
) => {
  let videoConstrainedWidth = videoEndX -. previewX
  let timelineConstrainedWidth = endX -. previewX
  let clipWidthFloat = Js.Math.min(videoConstrainedWidth, timelineConstrainedWidth)
  let clipWidth = clipWidthFloat->Float.toInt

  if previewX < videoEndX && clipWidth > 0 {
    ctx->Canvas2d.save

    let frameExtendsPastVideoEnd = previewX +. clipWidthFloat >= videoEndX
    let isActualLastFrame = frameNumber >= editorContext.videoMeta.durationInFrames - 1

    let isFirstFrame = i === 0
    let isLastFrame = isActualLastFrame || frameExtendsPastVideoEnd

    ctx->renderRoundedCorners(
      ~x=previewX,
      ~y=timeline_margin_y->Float.fromInt,
      ~width=clipWidth->Float.fromInt,
      ~height=Float.fromInt(scene_height_size),
      ~topLeft=if isFirstFrame {
        12.0
      } else {
        0.0
      },
      ~bottomLeft=if isFirstFrame {
        12.0
      } else {
        0.0
      },
      ~topRight=if isLastFrame {
        12.0
      } else {
        0.0
      },
      ~bottomRight=if isLastFrame {
        12.0
      } else {
        0.0
      },
      (),
    )

    ctx->Canvas2d.clip

    ctx->drawImage(
      ~imageData=image,
      ~dy=timeline_margin_y,
      ~dx=previewX->Float.toInt,
      ~dirtyHeight=scene_height_size,
      ~dirtyWidth=renderWidth,
    )

    ctx->Canvas2d.restore
  }
}

let renderMainScene = (ctx, size, editorContext: EditorContext.editorContext) => {
  let aspectRatio =
    editorContext.videoMeta.width->Float.fromInt /. editorContext.videoMeta.height->Float.fromInt

  let width = (Float.fromInt(scene_height_size) *. aspectRatio)->Utils.Math.floor

  let frameWidth = width->Float.fromInt
  let pixelsPerFrame = size.frameToPxRatio
  let startX = size.timelineMarginLeft->Float.fromInt

  let videoEndX = frameToX(editorContext.videoMeta.durationInFrames - 1, size)
  // Use the already calculated margins from the size object
  // The right margin is independently calculated and applied
  let rightMargin = size.timelineMarginRight->Float.fromInt

  // Calculate the actual viewport end with margin applied
  let viewportEnd = size.viewportOffset +. size.maxSceneWidth
  let endX = startX +. size.maxSceneWidth

  // Always fill the full width with preview frames
  // Calculate frame density based on zoom level
  let frameStep = if pixelsPerFrame > frameWidth {
    // When zoomed in: show every frame (or every few frames)
    1
  } else {
    // When zoomed out: skip frames to maintain good coverage
    (frameWidth /. pixelsPerFrame)->Js.Math.ceil->Float.toInt
  }

  let effectiveFrameWidth = if pixelsPerFrame > frameWidth {
    pixelsPerFrame
  } else {
    frameWidth
  }

  let adjustedWidth = viewportEnd -. startX -. rightMargin
  let numPreviews = (adjustedWidth /. effectiveFrameWidth)->Js.Math.ceil->Float.toInt + 1

  let firstVisibleFrame = (size.viewportOffset /. pixelsPerFrame)->Js.Math.floor->Float.toInt
  let adjustedFirstFrame = if firstVisibleFrame < 0 {
    0
  } else {
    firstVisibleFrame
  }

  Range.forEach(0, numPreviews, i => {
    // Calculate frame number starting from the first visible frame
    let frameForThisPreview = adjustedFirstFrame + i * frameStep
    let frameNumber = if frameForThisPreview >= editorContext.videoMeta.durationInFrames {
      editorContext.videoMeta.durationInFrames - 1
    } else if frameForThisPreview < 0 {
      0
    } else {
      frameForThisPreview
    }

    // Position preview to eliminate gaps at all zoom levels
    let previewX = if pixelsPerFrame > frameWidth {
      // When zoomed in: position frames based on actual timeline position
      frameToX(frameNumber, size)
    } else {
      // When zoomed out: use continuous positioning to avoid gaps
      let startX = frameToX(adjustedFirstFrame, size)
      startX +. i->Float.fromInt *. frameWidth
    }

    // Only render frames that are within the video duration and timeline bounds
    // videoEndX already calculated above

    // Calculate the minimum end position (either video end or timeline end)
    let actualEndX = Js.Math.min(videoEndX, endX)

    if (
      previewX >= startX -. frameWidth &&
      previewX < actualEndX &&
      // Ensure frame starts before the actual end
      frameNumber < editorContext.videoMeta.durationInFrames
    ) {
      // Create cache key for this frame
      let cacheKey = `${frameNumber->Belt.Int.toString}_${width->Belt.Int.toString}_${scene_height_size->Belt.Int.toString}`

      // Check if we have this frame cached
      let cachedImage = previewImageCache.contents->Belt.Map.String.get(cacheKey)

      switch cachedImage {
      | Some(image) => {
          // Use appropriate width based on zoom level to eliminate gaps
          let baseRenderWidth = if pixelsPerFrame > frameWidth {
            // At high zoom: stretch frame to fill timeline space
            pixelsPerFrame->Float.toInt
          } else {
            // At normal zoom: add 1 pixel overlap to prevent gaps
            width + 1
          }

          // Use full frame width but create clipping region that respects video end
          let renderWidth = baseRenderWidth

          // Use unified rendering function
          renderFrameWithClipping(
            ctx,
            ~image,
            ~previewX,
            ~renderWidth,
            ~frameNumber,
            ~i,
            ~videoEndX,
            ~endX,
            ~editorContext,
          )
        }
      | None => {
          // Generate new frame and cache it
          let svg =
            editorContext.wasmController->WasmController.render_preview_frame(
              frameNumber->Js.BigInt.fromInt,
            )

          let image = Image.make(~width, ~height=scene_height_size)

          image->Image.setSrc(svg->Image.btoa |> Js.String.concat("data:image/svg+xml;base64,"))
          image->Image.onLoad(() => {
            // Cache the image for future use and cleanup if needed
            previewImageCache := previewImageCache.contents->Belt.Map.String.set(cacheKey, image)
            cleanupCache()

            // Use appropriate width based on zoom level to eliminate gaps
            let baseRenderWidth = if pixelsPerFrame > frameWidth {
              // At high zoom: stretch frame to fill timeline space
              pixelsPerFrame->Float.toInt
            } else {
              // At normal zoom: add 1 pixel overlap to prevent gaps
              width + 1
            }

            // Use full frame width but create clipping region that respects video end
            let renderWidth = baseRenderWidth

            // Use unified rendering function
            renderFrameWithClipping(
              ctx,
              ~image,
              ~previewX,
              ~renderWidth,
              ~frameNumber,
              ~i,
              ~videoEndX,
              ~endX,
              ~editorContext,
            )
          })
        }
      }
    }
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
  ~palette: Theme.palette,
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
  ctx->Canvas2d.setStrokeStyle(String, palette.textTertiary)

  while x.contents < audioSpaceWidth || position.contents < positionEnd {
    let pcm = audioInfo.fltpData->Web.Float32Array.at(position.contents)->Utils.Option.unwrapOr(0.0)

    let y = mid +. y0 +. pcm *. mid
    ctx->Canvas2d.lineTo(~x=x.contents, ~y)

    position := (position.contents->Float.fromInt +. sector)->Utils.Math.floor
    x := (x.contents +. step)->Js.Math.floor
  }

  ctx->Canvas2d.stroke
}

let renderAudioMap = (
  ctx,
  size,
  editorContext: EditorContext.editorContext,
  ~palette: Theme.palette,
) => {
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
      let originalWidth = Float.fromInt(track.end - track.start) *. size.frameToPxRatio

      // Constrain audio track end position to respect right margin
      let timelineEnd = size.timelineMarginLeft->Float.fromInt +. size.maxSceneWidth
      let constrainedEndX = Js.Math.min(x +. originalWidth, timelineEnd)
      let width = constrainedEndX -. x

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
      ctx->Canvas2d.font(Theme.canvasFont)
      ctx->Canvas2d.setFillStyle(String, palette.textSecondary)
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

      ctx->Canvas2d.setFillStyle(String, palette.softAlpha)
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
        ~palette,
      )
      ->ignore

      ctx->Canvas2d.closePath
      ctx->Canvas2d.restore

      startY + audio_height + audio_height / 2
    }, 32)->ignore)
}

let renderTimeSlots = (
  ctx,
  size,
  editorContext: EditorContext.editorContext,
  ~palette: Theme.palette,
) => {
  // Calculate zoom-aware time slot spacing
  let pixelsPerFrame = size.frameToPxRatio
  let pixelsPerSecond = pixelsPerFrame *. editorContext.videoMeta.fps->Float.fromInt

  // Determine appropriate time interval based on zoom level
  let (timeIntervalFrames, full_timestamp_each_steps) = if pixelsPerSecond > 200.0 {
    // Very zoomed in: show every 0.5 seconds
    (editorContext.videoMeta.fps / 2, 2)
  } else if pixelsPerSecond > 100.0 {
    // Zoomed in: show every second
    (editorContext.videoMeta.fps, 2)
  } else if pixelsPerSecond > 50.0 {
    // Normal: show every 2 seconds
    (editorContext.videoMeta.fps * 2, 2)
  } else if pixelsPerSecond > 20.0 {
    // Zoomed out: show every 5 seconds
    (editorContext.videoMeta.fps * 5, 2)
  } else {
    // Very zoomed out: show every 10 seconds
    (editorContext.videoMeta.fps * 10, 2)
  }

  // Calculate visible range based on viewport
  let startFrame = (size.viewportOffset /. pixelsPerFrame)->Float.toInt
  let endFrame = ((size.viewportOffset +. size.maxSceneWidth) /. pixelsPerFrame)->Float.toInt

  // Calculate first time slot to show (aligned to interval)
  let firstSlotFrame = startFrame / timeIntervalFrames * timeIntervalFrames - timeIntervalFrames
  let lastSlotFrame = endFrame + timeIntervalFrames

  // Render time slots for visible range - with safety guard
  let currentFrame = ref(firstSlotFrame)
  let maxIterations = 10000 // Safety limit to prevent infinite loops
  let iterations = ref(0)
  while (
    currentFrame.contents <= lastSlotFrame &&
    iterations.contents < maxIterations &&
    timeIntervalFrames > 0
  ) {
    let frame = currentFrame.contents
    if frame >= 0 && frame <= editorContext.videoMeta.durationInFrames {
      let x = frameToX(frame, size)

      ctx->Canvas2d.beginPath
      ctx->Canvas2d.moveTo(~x, ~y=0.)
      ctx->Canvas2d.lineTo(~x, ~y=18.)

      ctx->Canvas2d.setStrokeStyle(String, palette.border)
      ctx->Canvas2d.lineWidth(1.0)
      ctx->Canvas2d.stroke

      // Show timestamp every few slots based on zoom level
      if mod(frame / timeIntervalFrames, full_timestamp_each_steps) === 0 {
        ctx->Canvas2d.font(Theme.canvasFont)
        ctx->Canvas2d.setFillStyle(String, palette.textTertiary)

        frame
        ->Float.fromInt
        ->Utils.Math.divideFloat(editorContext.videoMeta.fps->Float.fromInt)
        ->Utils.Duration.formatSeconds
        ->Canvas2d.fillText(ctx, ~x=x +. 8., ~y=14.)
      }
    }

    currentFrame := currentFrame.contents + timeIntervalFrames
    iterations := iterations.contents + 1
  }
}

@react.component
let make = (~size: canvasSize) => {
  let canvasRef = React.useRef(Js.Nullable.null)
  let editorContext = EditorContext.useEditorContext()
  let (player, _) = editorContext.usePlayer()
  let theme = Theme.use()
  let palette = Theme.palette(theme)

  let (throttledViewportOffset, _) = UseDebounce.useThrottle(player.viewportOffset, ~ms=16)
  let (posterVersion, setPosterVersion) = React.useState(() => 0)
  useCanvasScale(canvasRef, size)

  // Listen for poster frame ready events to invalidate cached broken previews
  React.useEffect0(() => {
    let handlePosterReady = _ => {
      previewImageCache := Belt.Map.String.empty
      setPosterVersion(v => v + 1)
    }

    Dom.window
    |> DocumentEvent.asEventTarget
    |> Dom.EventTarget.addEventListener("fframes-poster-ready", handlePosterReady)

    Some(
      () =>
        Dom.window
        |> DocumentEvent.asEventTarget
        |> Dom.EventTarget.removeEventListener("fframes-poster-ready", handlePosterReady),
    )
  })

  // Separate effect for fast elements (time slots, scenes, audio)
  React.useEffect3(() => {
    canvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(element => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(element)

      // Clear only the necessary area instead of full canvas
      ctx->Canvas2d.clearRect(~x=0., ~y=0., ~w=size.scaledWidth, ~h=size.scaledHeight)

      ctx->renderTimeSlots(size, editorContext, ~palette)
      ctx->renderScenes(size, editorContext, ~palette)
      ctx->Canvas2d.save
      ctx->renderAudioMap(size, editorContext, ~palette)
      ctx->Canvas2d.restore

      ()
    })
    ->ignore

    None
  }, (size, player.viewportOffset, theme))

  React.useEffect3(() => {
    canvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(element => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(element)
      ctx->renderMainScene(size, editorContext)
      ()
    })
    ->ignore

    None
  }, (size, throttledViewportOffset, posterVersion))

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
