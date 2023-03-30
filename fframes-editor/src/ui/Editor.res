open Hooks

@genType
let a = Js.Dict.empty

@genType.as("Editor") @react.component
let make = () => {
  let context = EditorContext.useEditorContext()
  let (player, _) = context.usePlayer()
  let (isFullScreen, fullScreenToggler) = Hooks.useToggle(false)
  let layout = useEditorLayout(~isFullScreen)
  let previewRef = React.useRef(Js.Nullable.null)
  let (listVariant, setListVariant) = React.useState(_ =>
    switch context.options.mediaListLayout {
    | #grid => MediaList.Grid
    | #list => MediaList.List
    | #fromAspectRatio =>
      if layout.preview.width /. layout.preview.height > 1.3 {
        MediaList.List
      } else {
        MediaList.Grid
      }
    }
  )

  let videoTitle = React.useMemo1(() => {
    switch context.videoMeta.name->Js.String.split("::")->Utils.Array.last {
    | Some(name) => React.string(name)
    | _ => React.string("Unknown video")
    }
  }, [context.videoMeta])

  React.useEffect0(() => {
    previewRef.current
    ->Js.Nullable.toOption
    ->Belt.Option.forEach(Js.Console.log2("Happy video hacking! Your preview will be rendered at"))

    None
  })

  <div id="fframes-editor" className="w-screen h-screen bg-gray-900 overflow-hidden relative">
    <ReactHelmet>
      <title> {videoTitle} </title>
      <style type_="text/css">
        {React.string(
          `
            #editor-preview > svg {
              transform-origin: top left !important;
              transform: scale(${layout.preview.scale->Js.Float.toString}) !important
            }
          `,
        )}
      </style>
    </ReactHelmet>
    <div className="overflow-auto flex justify-center w-full">
      {
        // MediaList
        layout.mediaControls
        ->Belt.Option.map(size =>
          <div
            style={size->UseEditorLayout.sizeToStyle}
            className="col-span-2 h-full overflow-auto flex flex-col p-6 border-r border-gray-800">
            <div className="flex items-center justify-between mb-6 pt-4">
              <h1 className="text-3xl mt-px font-medium text-white"> {videoTitle} </h1>
              <div className="isolate flex rounded-md shadow-sm">
                <button
                  type_="button"
                  onClick={_ => setListVariant(_ => MediaList.List)}
                  className={Cx.cx([
                    "transition-colors relative inline-flex items-center rounded-l-md px-2 py-2 text-gray-800 ring-1 ring-inset ring-gray-800 hover:bg-gray-50 focus:z-10",
                    switch listVariant {
                    | MediaList.List => "bg-slate-100"
                    | MediaList.Grid => "bg-slate-300"
                    },
                  ])}>
                  <span className="sr-only"> {React.string("List view")} </span>
                  <Icons.ListViewIcon color="currentColor" className="h-4 w-5" />
                </button>
                <button
                  type_="button"
                  onClick={_ => setListVariant(_ => MediaList.Grid)}
                  className={Cx.cx([
                    "transition-colors relative -ml-px inline-flex items-center rounded-r-md px-2 py-2 text-gray-800 ring-1 ring-inset ring-gray-800 hover:bg-gray-50 focus:z-10",
                    switch listVariant {
                    | MediaList.List => "bg-slate-300"
                    | MediaList.Grid => "bg-slate-100"
                    },
                  ])}>
                  <span className="sr-only"> {React.string("Grid view")} </span>
                  <Icons.GridViewIcon color="currentColor" className="h-5 w-5" />
                </button>
              </div>
            </div>
            <MediaList variant=listVariant />
          </div>
        )
        ->Utils.Option.unwrapOr(React.null)
      }
      // Preview
      <div
        id="editor-preview"
        ref={ReactDOM.Ref.domRef(previewRef)}
        style={layout.preview->UseEditorLayout.sizeToStyle}
        className="bg-black">
        {player.svg
        ->Utils.Option.unwrapOr("")
        ->VDom.parseReactElement({
          htmlparser2: {
            xmlMode: true,
          },
        })}
      </div>
    </div>
    {layout.timeLine
    ->Belt.Option.map(sectionSize =>
      <div
        style={sectionSize->UseEditorLayout.sizeToStyle} className="shadow-lg w-screen bg-gray-800">
        <Timeline sectionSize />
      </div>
    )
    ->Utils.Option.unwrapOr(React.null)}
    <Dock fullScreenToggler />
  </div>
}
