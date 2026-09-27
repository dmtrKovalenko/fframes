@react.component
let make = () => {
  let editorContext = EditorContext.useEditorContext()
  let (player, dispatch) = editorContext.usePlayer()

  let handleZoomIn = Hooks.useEvent(_ => {
    let zoomFactor = ZoomUtils.calculateZoomFactorFromDelta(
      -1.0,
      ~currentZoom=player.zoom,
      ~sensitivity=1.0,
      (),
    )
    let newZoom = player.zoom *. zoomFactor

    // Need access to timeline size for proper viewport calculation
    // For now, just use simple zoom without viewport adjustment
    dispatch(Player.SetZoom(newZoom))
  })

  let handleZoomOut = Hooks.useEvent(_ => {
    let zoomFactor = ZoomUtils.calculateZoomFactorFromDelta(
      1.0,
      ~currentZoom=player.zoom,
      ~sensitivity=1.0,
      (),
    )
    let newZoom = player.zoom *. zoomFactor

    // If zooming out to 100% or less, reset viewport to show entire timeline
    if newZoom <= 1.0 {
      dispatch(Player.BatchZoomUpdate(newZoom, 0.0))
    } else {
      // For button-based zoom, just use simple zoom without viewport adjustment
      dispatch(Player.SetZoom(newZoom))
    }
  })

  let handleZoomReset = Hooks.useEvent(_ => {
    dispatch(Player.BatchZoomUpdate(1., 0.0))
  })

  <div role="group" ariaLabel="Timeline zoom" className="flex items-center">
    <IconButton onClick=handleZoomOut label="Zoom out" shortcut="Scroll">
      <Icons.MinusIcon />
    </IconButton>
    <Tooltip content={React.string("Fit timeline")}>
      <button
        type_="button"
        onClick=handleZoomReset
        className={Cx.cx([
          "h-9 min-w-[3.25rem] rounded-full px-2 text-xs font-medium text-secondary tabular transition-colors duration-150 hover:bg-primary-ghost-hover hover:text-default",
          IconButton.focusRing,
        ])}>
        {React.string(`${(player.zoom *. 100.0)->Js.Float.toFixedWithPrecision(~digits=0)}%`)}
      </button>
    </Tooltip>
    <IconButton onClick=handleZoomIn label="Zoom in" shortcut="Scroll">
      <Icons.PlusIcon />
    </IconButton>
  </div>
}
