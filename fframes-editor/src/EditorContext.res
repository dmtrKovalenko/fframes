open Webapi

module DocumentEvent = Dom.EventTarget.Impl(Dom.Window)

@genType
type editorContext = {
  wasmController: WasmController.t,
  videoMeta: WasmController.videoMeta,
  options: WasmController.options,
  usePlayer: unit => (Player.state, Player.action => unit),
}

let editorContext = React.createContext(None)
let providerElement = React.Context.provider(editorContext)

let useEditorContext = () => {
  let context = React.useContext(editorContext)

  switch context {
  | Some(context) => context
  | _ => failwith("Missing editorContext.")
  }
}

module MakeEditorContext = (Wasm: WasmController.WasmBridge) => {
  module PlayerObserver = Player.MakePlayer(Wasm)

  @react.component @genType
  let make = (~children) => {
    React.useLayoutEffect0(() => {
      Some(
        MediaLoader.MediaLoaderObserver.subscribe(state => {
          let player = PlayerObserver.get()

          if state.allMediaLoaded && player.playState === CantPlay {
            PlayerObserver.dispatch(AllowPlay)
            PlayerObserver.dispatch(NewFrame(player.frame))
          }
        }),
      )
    })

    let usePlayer = () => {
      (PlayerObserver.useObservable(), PlayerObserver.dispatch)
    }

    React.createElement(
      providerElement,
      {
        "value": Some({
          wasmController: Wasm.controller,
          videoMeta: Wasm.videoMeta,
          usePlayer: usePlayer,
          options: Wasm.options,
        }),
        "children": children,
      },
    )
  }
}

@genType.as("EditorContext")
let makeEditorContextComponent = (
  ~wasmController: WasmController.t,
  ~videoMeta: WasmController.videoMeta,
  ~options: WasmController.options,
) => {
  module Bridge = {
    let controller = wasmController
    let videoMeta = videoMeta
    let options = options
  }

  module Context = MakeEditorContext(Bridge)

  @react.component
  let make = (~children) => {
    <Context> {children} </Context>
  }

  make
}
