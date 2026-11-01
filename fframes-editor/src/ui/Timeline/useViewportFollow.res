open Belt
open CanvasSize

// Custom hook to handle viewport following for any frame navigation
let useViewportFollow = (sectionSize: UseEditorLayout.sectionSize) => {
  let editorContext = EditorContext.useEditorContext()
  let (player, dispatch) = editorContext.usePlayer()

  let followFrameToViewport = React.useCallback2((targetFrame: int) => {
    // Calculate the same size object as Timeline does
    let scale = Web.Window.devicePixelRatio
    // First calculate margins to determine available timeline space
    let initialBasePxRatio =
      sectionSize.width /. editorContext.videoMeta.durationInFrames->Js.Int.toFloat
    let initialVideoTotalWidth =
      editorContext.videoMeta.durationInFrames->Js.Int.toFloat *.
        (initialBasePxRatio *.
        player.zoom)
    let (timelineMarginLeft, timelineMarginRight) = calculateTimelineMargins(
      player.viewportOffset,
      ~videoTotalWidth=initialVideoTotalWidth,
      ~visibleWidth=sectionSize.width,
    )

    // Calculate available width after reserving margin space
    let availableWidth =
      sectionSize.width -. timelineMarginLeft->Float.fromInt -. timelineMarginRight->Float.fromInt

    // Base timeline scaling on available width, not full width
    let basePxRatio = availableWidth /. editorContext.videoMeta.durationInFrames->Js.Int.toFloat
    let frameToPxRatio = basePxRatio *. player.zoom
    let maxSceneWidth = availableWidth

    let size = {
      width: sectionSize.width,
      height: sectionSize.height,
      scaledWidth: sectionSize.width *. scale,
      scaledHeight: sectionSize.height *. scale,
      scale: scale,
      maxSceneWidth: maxSceneWidth,
      frameToPxRatio: frameToPxRatio,
      pxToFrameRatio: 1. /. frameToPxRatio,
      viewportOffset: player.viewportOffset,
      timelineMarginLeft: timelineMarginLeft,
      timelineMarginRight: timelineMarginRight,
      lanesTop: lanesTop(editorContext.videoMeta.scenesTimeline),
    }

    // Always move viewport to position frame at start
    // Calculate content bounds to prevent scrolling beyond end
    let totalContentWidth =
      editorContext.videoMeta.durationInFrames->Js.Int.toFloat *. size.frameToPxRatio
    let maxOffset = if totalContentWidth > size.maxSceneWidth {
      totalContentWidth -. size.maxSceneWidth
    } else {
      0.0
    }

    // Position target frame at the start of the screen
    // For frameToX to return 0: frame * frameToPxRatio + marginLeft - viewportOffset = 0
    // So: viewportOffset = frame * frameToPxRatio + marginLeft
    let framePosition = targetFrame->Js.Int.toFloat *. size.frameToPxRatio
    let rawOffset = framePosition

    // Constrain offset to valid bounds
    let constrainedOffset = if rawOffset < 0.0 {
      0.0
    } else if rawOffset > maxOffset {
      maxOffset
    } else {
      rawOffset
    }

    dispatch(Player.SetViewportOffset(constrainedOffset))
  }, (player.viewportOffset, player.zoom))

  followFrameToViewport
}
