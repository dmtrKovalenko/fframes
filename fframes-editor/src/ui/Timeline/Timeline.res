open Belt
open CanvasSize

@react.component
let make = (~sectionSize: UseEditorLayout.sectionSize) => {
  let editorContext = EditorContext.useEditorContext()
  let (player, dispatch) = editorContext.usePlayer()

  let size = React.useMemo6(() => {
    let scale = Web.Window.devicePixelRatio
    let initialBasePxRatio =
      sectionSize.width /. editorContext.videoMeta.durationInFrames->Float.fromInt
    let initialVideoTotalWidth =
      editorContext.videoMeta.durationInFrames->Float.fromInt *. (initialBasePxRatio *. player.zoom)
    let (timelineMarginLeft, timelineMarginRight) = calculateTimelineMargins(
      player.viewportOffset,
      ~videoTotalWidth=initialVideoTotalWidth,
      ~visibleWidth=sectionSize.width,
    )

    // Calculate available width after reserving margin space
    let availableWidth =
      sectionSize.width -. timelineMarginLeft->Float.fromInt -. timelineMarginRight->Float.fromInt

    // Base timeline scaling on available width, not full width
    let basePxRatio = availableWidth /. editorContext.videoMeta.durationInFrames->Float.fromInt
    let frameToPxRatio = basePxRatio *. player.zoom
    let maxSceneWidth = availableWidth

    {
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
  }, (
    sectionSize.height,
    sectionSize.width,
    sectionSize.scale,
    editorContext.videoMeta.durationInFrames,
    player.zoom,
    player.viewportOffset,
  ))

  // Auto-follow the current frame during playback
  React.useEffect3(() => {
    if player.playState === Playing {
      // Recalculate size with current viewport offset to avoid stale values
      let currentSize = {
        ...size,
        viewportOffset: player.viewportOffset,
      }
      let currentFrameX = frameToX(player.frame, currentSize)

      // Only move if current frame is actually outside the visible area
      if currentFrameX < 0.0 || currentFrameX > currentSize.maxSceneWidth {
        // Calculate content bounds to prevent scrolling beyond end
        let totalContentWidth =
          editorContext.videoMeta.durationInFrames->Float.fromInt *. currentSize.frameToPxRatio
        let maxOffset = if totalContentWidth > currentSize.maxSceneWidth {
          totalContentWidth -. currentSize.maxSceneWidth
        } else {
          0.0
        }

        // Position the frame at the start of the viewport
        let framePosition = player.frame->Float.fromInt *. currentSize.frameToPxRatio
        let rawOffset = framePosition

        let constrainedOffset = if rawOffset < 0.0 {
          0.0
        } else if rawOffset > maxOffset {
          maxOffset
        } else {
          rawOffset
        }

        dispatch(Player.SetViewportOffset(constrainedOffset))
      }
    }
    None
  }, (player.frame, player.playState, player.zoom))

  <>
    <div className="relative">
      {switch player.playState {
      | CantPlay => React.null
      | _ => <SceneMapCanvas size />
      }}
      <ControlsCanvas size />
      <SeekBarCanvas size />
    </div>
  </>
}
