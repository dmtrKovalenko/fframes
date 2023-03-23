open UseDimensions

let min_timeline_height = 450
let min_media_controls_width = 370

type sectionSize = {
  height: float,
  width: float,
  scale: float,
}

let emptySize: sectionSize = {
  height: 0.0,
  width: 0.0,
  scale: 0.0,
}

let sizeToStyle = ({width, height}: sectionSize) => {
  ReactDOMStyle.make(
    ~width=`${width->Belt.Float.toString}px`,
    ~height=`${height->Belt.Float.toString}px`,
    (),
  )
}

type editorLayout = {
  preview: sectionSize,
  timeLine: option<sectionSize>,
  mediaControls: option<sectionSize>,
}

let calculatePreviewSize = (
  windowDimensions: dimensions,
  {width, height}: WasmController.videoMeta,
  ~min_media_controls_width,
  ~min_timeline_height,
) => {
  let max_preview_width = windowDimensions.width - min_media_controls_width
  let max_preview_height = windowDimensions.height - min_timeline_height

  if width > max_preview_width || height > max_preview_height {
    let scale = min(
      max_preview_width->float_of_int /. width->float_of_int,
      max_preview_height->float_of_int /. height->float_of_int,
    )

    {
      width: width->Belt.Int.toFloat *. scale,
      height: height->Belt.Int.toFloat *. scale,
      scale: scale,
    }
  } else {
    {
      scale: 1.0,
      width: width->Belt.Int.toFloat,
      height: height->Belt.Int.toFloat,
    }
  }
}

let useEditorLayout = (~isFullScreen) => {
  let viewportSize = useDimensions()
  let {videoMeta} = EditorContext.useEditorContext()

  React.useMemo3(() => {
    if isFullScreen {
      {
        preview: calculatePreviewSize(
          viewportSize,
          videoMeta,
          ~min_media_controls_width=0,
          ~min_timeline_height=0,
        ),
        mediaControls: None,
        timeLine: None,
      }
    } else {
      viewportSize
      ->calculatePreviewSize(videoMeta, ~min_media_controls_width, ~min_timeline_height)
      ->(
        previewSize => {
          preview: previewSize,
          mediaControls: Some({
            width: viewportSize.width->Belt.Int.toFloat -. previewSize.width,
            height: previewSize.height,
            scale: 1.0,
          }),
          timeLine: Some({
            height: viewportSize.height->Belt.Int.toFloat -. previewSize.height,
            width: viewportSize.width->Belt.Int.toFloat,
            scale: 1.0,
          }),
        }
      )
    }
  }, (viewportSize.height, viewportSize.width, isFullScreen))
}
