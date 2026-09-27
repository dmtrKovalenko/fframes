open Belt
open CanvasSize

module Canvas = Webapi.Canvas
module Canvas2d = Webapi.Canvas.Canvas2d

@get external getClientX: ReactEvent.Wheel.t => int = "clientX"
@get external getShiftKey: ReactEvent.Wheel.t => bool = "shiftKey"
@get external getCtrlKey: ReactEvent.Wheel.t => bool = "ctrlKey"
@get external getDeltaX: ReactEvent.Wheel.t => float = "deltaX"

let renderSeekBar = (ctx, size, playState: Player.state, ~palette: Theme.palette) => {
  let x = frameToX(playState.frame, size)

  ctx->Canvas2d.setStrokeStyle(String, palette.accent)
  ctx->Canvas2d.setFillStyle(String, palette.accent)
  ctx->Canvas2d.lineWidth(2.)

  ctx->Canvas2d.beginPath
  ctx->Canvas2d.moveTo(~x, ~y=0.)
  ctx->Canvas2d.lineTo(~x, ~y=size.height)
  ctx->Canvas2d.stroke

  // Playhead knob
  ctx->Canvas2d.beginPath
  ctx->Canvas2d.arc(~x, ~y=5., ~r=5., ~startAngle=0., ~endAngle=6.283185307179586, ~anticw=false)
  ctx->Canvas2d.fill
}

let calculateFrameFromEvent = (e, ~size, ~viewportOffset) => {
  let rectLeft =
    e
    ->ReactEvent.Mouse.target
    ->Web.Element.targetAsElement
    ->Webapi.Dom.Element.getBoundingClientRect
    ->Webapi.Dom.DomRect.left
    ->Int.fromFloat

  let x = e->ReactEvent.Mouse.clientX - rectLeft - size.timelineMarginLeft

  // Constrain cursor interaction to timeline bounds (0 to maxSceneWidth)
  if x > 0 && x->Float.fromInt <= size.maxSceneWidth {
    ((x->Float.fromInt +. viewportOffset) *. size.pxToFrameRatio)->Int.fromFloat
  } else if x->Float.fromInt > size.maxSceneWidth {
    // Cursor beyond right boundary - use rightmost frame
    ((size.maxSceneWidth +. viewportOffset) *. size.pxToFrameRatio)->Int.fromFloat
  } else {
    // Cursor beyond left boundary - use leftmost frame
    (viewportOffset *. size.pxToFrameRatio)->Int.fromFloat
  }
}

let handleHorizontalScroll = (
  e,
  dispatch,
  player: Player.state,
  size,
  editorContext: EditorContext.editorContext,
) => {
  // Get horizontal scroll delta (from shift+wheel or trackpad)
  let deltaX = getDeltaX(e)
  let deltaY = ReactEvent.Wheel.deltaY(e)

  if deltaX != 0.0 || getShiftKey(e) {
    ReactEvent.Wheel.preventDefault(e)
    ReactEvent.Wheel.stopPropagation(e)
  }

  // Use deltaX if available (trackpad horizontal scroll), otherwise use deltaY with shift
  let scrollDelta = if deltaX != 0.0 {
    deltaX
  } else if getShiftKey(e) {
    deltaY // Shift+vertical wheel becomes horizontal scroll
  } else {
    0.0
  }

  if scrollDelta != 0.0 {
    // Calculate scroll sensitivity (adjust as needed)
    let scrollSensitivity = 1.0
    let scrollAmount = scrollDelta *. scrollSensitivity

    // Calculate new viewport offset
    let newOffset = player.viewportOffset +. scrollAmount

    // Calculate content bounds to constrain scrolling
    let totalContentWidth =
      editorContext.videoMeta.durationInFrames->Float.fromInt *. size.frameToPxRatio
    let minOffset = 0.0
    let maxOffset = if totalContentWidth > size.maxSceneWidth {
      totalContentWidth -. size.maxSceneWidth
    } else {
      0.0
    }

    // Constrain offset to valid bounds
    let constrainedOffset = if newOffset < minOffset {
      minOffset
    } else if newOffset > maxOffset {
      maxOffset
    } else {
      newOffset
    }

    dispatch(Player.SetViewportOffset(constrainedOffset))
    true // Indicate we handled the scroll
  } else {
    false // Indicate we didn't handle the scroll
  }
}

let handleWheelZoom = (
  e,
  dispatch,
  player: Player.state,
  size,
  editorContext: EditorContext.editorContext,
) => {
  let clientX = e->getClientX
  let rectLeft =
    e
    ->ReactEvent.Wheel.target
    ->Web.Element.targetAsElement
    ->Webapi.Dom.Element.getBoundingClientRect
    ->Webapi.Dom.DomRect.left
    ->Int.fromFloat

  let mouseX = clientX - rectLeft - size.timelineMarginLeft
  let mouseXFloat = mouseX->Float.fromInt

  let ctrlKey = e->getCtrlKey
  let shiftKey = e->getShiftKey
  let sensitivity = ZoomUtils.getZoomSensitivity(~ctrlKey, ~shiftKey)

  let deltaY = ReactEvent.Wheel.deltaY(e)
  let zoomFactor = ZoomUtils.calculateZoomFactorFromDelta(
    deltaY,
    ~currentZoom=player.zoom,
    ~sensitivity,
    (),
  )
  let newZoom = Player.validateZoom(player.zoom *. zoomFactor)

  // Calculate new viewport offset to keep current frame (seek bar) position fixed
  let newViewportOffset = ZoomUtils.calculateViewportOffsetForCurrentFrameZoom(
    ~currentFrame=player.frame,
    ~currentZoom=player.zoom,
    ~newZoom,
    ~currentViewportOffset=player.viewportOffset,
    ~maxSceneWidth=size.maxSceneWidth,
    ~totalFrames=editorContext.videoMeta.durationInFrames,
    ~timelineMarginLeft=size.timelineMarginLeft->Js.Int.toFloat,
  )

  // Use batched update to minimize re-renders
  if mouseX >= 0 && mouseXFloat <= size.maxSceneWidth {
    dispatch(Player.BatchZoomUpdate(newZoom, newViewportOffset))
  } else {
    // Mouse outside bounds - just zoom without changing offset
    dispatch(Player.SetZoom(newZoom))
  }
}

let handleWheel = (
  e,
  dispatch,
  player: Player.state,
  size,
  editorContext: EditorContext.editorContext,
) => {
  let deltaX = getDeltaX(e)
  let isHorizontalIntent = Js.Math.abs(deltaX) > 0.1 || getShiftKey(e)

  if isHorizontalIntent {
    let _ = handleHorizontalScroll(e, dispatch, player, size, editorContext)
  } else {
    handleWheelZoom(e, dispatch, player, size, editorContext)
  }
}

@react.component
let make = (~size) => {
  let seekCanvasRef = React.useRef(Js.Nullable.null)
  let editorContext = EditorContext.useEditorContext()
  let (player, dispatch) = editorContext.usePlayer()
  let theme = Theme.use()

  useCanvasScale(seekCanvasRef, size)

  React.useEffect4(() => {
    seekCanvasRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.map(canvasElement => {
      let ctx = Webapi.Canvas.CanvasElement.getContext2d(canvasElement)
      ctx->Canvas2d.clearRect(~x=0., ~y=0., ~w=size.scaledWidth, ~h=size.scaledHeight)

      switch player.playState {
      | CantPlay => ()
      | _ => renderSeekBar(ctx, size, player, ~palette=Theme.palette(theme))
      }->ignore
    })
    ->ignore

    None
  }, (size, player.frame, player.playState, theme))

  let handleMouseMove = Hooks.useEvent(e => {
    if player.playState !== Playing && Webapi.Dom.document->Web.Document.hasFocus {
      dispatch(NewFrame(calculateFrameFromEvent(e, ~size, ~viewportOffset=player.viewportOffset)))
    }
  })

  let handleClick = Hooks.useEvent(e => {
    let frame = calculateFrameFromEvent(e, ~size, ~viewportOffset=player.viewportOffset)

    dispatch(Seek(frame))
    dispatch(Play)
  })

  <canvas
    onClick=handleClick
    onMouseMove=handleMouseMove
    onWheel={e => handleWheel(e, dispatch, player, size, editorContext)}
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
