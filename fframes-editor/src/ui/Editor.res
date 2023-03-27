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

  <div className="w-screen h-screen bg-gray-900">
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
            className="col-span-2 h-full overflow-auto flex flex-col py-6 border-r border-gray-800">
            <h1 className="text-2xl mb-6 font-medium text-white px-6"> {videoTitle} </h1>
            <MediaList />
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
