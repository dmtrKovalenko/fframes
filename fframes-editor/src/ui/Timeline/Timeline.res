open Belt
open CanvasSize

@react.component
let make = (~sectionSize: UseEditorLayout.sectionSize) => {
  let editorContext = EditorContext.useEditorContext()
  let (player, _) = editorContext.usePlayer()

  let size = React.useMemo4(() => {
    let scale = Web.Window.devicePixelRatio
    let maxSceneWidth = sectionSize.width -. timeline_margin_x->Float.fromInt
    let frameToPxRatio = maxSceneWidth /. editorContext.videoMeta.durationInFrames->Float.fromInt

    {
      width: sectionSize.width,
      height: sectionSize.height,
      scaledWidth: sectionSize.width *. scale,
      scaledHeight: sectionSize.height *. scale,
      scale: scale,
      maxSceneWidth: maxSceneWidth,
      frameToPxRatio: frameToPxRatio,
      pxToFrameRation: 1. /. frameToPxRatio,
    }
  }, (
    sectionSize.height,
    sectionSize.width,
    sectionSize.scale,
    editorContext.videoMeta.durationInFrames,
  ))

  <div className="relative">
    {switch player.playState {
    | CantPlay => React.null
    | _ => <SceneMapCanvas size />
    }}
    <ControlsCanvas size />
    <SeekBarCanvas size />
  </div>
}
