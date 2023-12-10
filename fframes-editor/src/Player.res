open Belt

type playState = Playing | Paused | WaitingForAction | CantPlay

/// This state should only contain a state that changes that affect the animation runtime or will likely change 60 t/s
@genType
type state = {
  frame: int,
  startPlayingFrame: int,
  playState: playState,
  fpsLimit: option<int>,
  svg: option<string>,
  volume: option<int>,
  magnet: option<int>,
}

@genType
type action =
  | Seek(int)
  | NewFrame(int)
  | AllowPlay
  | Play
  | Pause
  | SetVolume(int)
  | SetMagnet

let currentFps: ref<option<int>> = ref(None)

@inline
let volume_key = "ffvolume"
@inline
let frame_key = "fframe"
@inline
let scene_key = "ffscene"
let get_magnet_key = (video: WasmController.videoMeta) => video.name ++ "_ffmagnet"

let min_volume = 0
let max_volume = 100
let validateVolume = Utils.Math.minMax(~min=min_volume, ~max=max_volume)

module MakePlayer = (Wasm: WasmController.WasmBridge) => {
  module PlayerState = {
    type t = state
    open WasmController

    let sceneIndex =
      Dom.Storage.getItem(scene_key, Dom.Storage.localStorage)->Option.flatMap(Js.Int.fromString)

    let savedMagnet =
      Dom.Storage.getItem(get_magnet_key(Wasm.videoMeta), Dom.Storage.localStorage)->Option.flatMap(
        Js.Int.fromString,
      )

    let initialFrame = switch (
      savedMagnet,
      Wasm.videoMeta.scenesTimeline->Js.Nullable.toOption,
      sceneIndex,
    ) {
    | (Some(magnet), _, _) =>
      magnet->Utils.Math.minI(Wasm.videoMeta.durationInFrames)->Utils.Option.some
    | (_, Some(scenes), Some(index)) =>
      scenes->Js.Array.get(index)->Option.map(scene => scene.start)
    | _ =>
      Dom.Storage.getItem(frame_key, Dom.Storage.localStorage)->Option.flatMap(Js.Int.fromString)
    }->Utils.Option.unwrapOr(0)

    let volume = switch Dom.Storage.getItem(volume_key, Dom.Storage.localStorage) {
    | Some(savedValue) if Wasm.videoMeta.hasAudio => savedValue->Js.Int.fromString
    | None if Wasm.videoMeta.hasAudio => Some(60)
    | _ => None
    }

    let initial = switch MediaLoader.MediaLoaderObserver.get() {
    | state if state.allMediaLoaded => {
        frame: initialFrame,
        startPlayingFrame: initialFrame,
        playState: WaitingForAction,
        fpsLimit: Some(Wasm.videoMeta.fps),
        volume: volume,
        svg: Wasm.controller.render_frame(0->Js.BigInt.fromInt)->Utils.Option.some,
        magnet: savedMagnet,
      }
    | _ => {
        frame: initialFrame,
        startPlayingFrame: initialFrame,
        playState: CantPlay,
        svg: None,
        volume: volume,
        fpsLimit: Some(Wasm.videoMeta.fps),
        magnet: savedMagnet,
      }
    }
  }

  include UseObservable.Pubsub(PlayerState)

  let recordFrame = frame => {
    switch Wasm.videoMeta.scenesTimeline->Js.Nullable.toOption {
    | Some(scenes) => {
        let currentScene = scenes->Js.Array.findIndex(scene => scene.end > frame)
        Dom.Storage.localStorage |> Dom.Storage.setItem(scene_key, currentScene->Js.Int.toString)
      }
    | _ => Dom.Storage.localStorage |> Dom.Storage.setItem(frame_key, frame->Js.Int.toString)
    }
  }

  let reducer = action => {
    let state = get()
    switch action {
    | Seek(frame) | NewFrame(frame) if frame >= Wasm.videoMeta.durationInFrames || frame < 0 => {
        let frame = state.magnet->Utils.Option.unwrapOr(0)
        let svg = Wasm.controller.render_frame(frame->Js.BigInt.fromInt)

        {
          ...state,
          frame: frame,
          svg: Some(svg),
          playState: Wasm.options.loop ? Playing : Paused,
          startPlayingFrame: frame,
        }
      }
    | Seek(frame) | NewFrame(frame) => {
        let svg = Wasm.controller.render_frame(frame->Js.BigInt.fromInt)

        {
          ...state,
          frame: frame,
          svg: Some(svg),
          startPlayingFrame: switch action {
          | Seek(frame) => frame
          | _ => state.startPlayingFrame
          },
        }
      }
    | AllowPlay => {...state, playState: WaitingForAction}
    | Play if state.frame <= 0 || state.frame >= Wasm.videoMeta.durationInFrames => {
        ...state,
        frame: state.magnet->Utils.Option.unwrapOr(0),
        playState: Playing,
      }
    | Play => {...state, playState: Playing, startPlayingFrame: state.frame}
    | Pause => {...state, playState: Paused}
    | SetVolume(volume) => {
        ...state,
        volume: Some(volume),
      }
    | SetMagnet if state.magnet === Some(state.frame) => {
        ...state,
        magnet: None,
      }
    | SetMagnet => {
        ...state,
        magnet: Some(state.frame),
      }
    }
  }

  let onFrame = (dispatch, ~secondsFromStart) => {
    let nextFrame =
      (secondsFromStart *. Wasm.videoMeta.fps->Float.fromInt +.
        get().startPlayingFrame->Float.fromInt)->Utils.Math.floor

    if nextFrame !== get().frame {
      dispatch(NewFrame(nextFrame))
    }

    get().playState === Playing
  }

  let sideEffect = (action, dispatch) => {
    let startPlaying = currentFrame => {
      get().volume->Option.map(AnimationRuntime.AudioRuntime.setVolume)->ignore
      AnimationRuntime.AudioRuntime.startAnimation(
        ~onFrame=onFrame(dispatch),
        ~currentFrame,
        ~videoMeta=Wasm.videoMeta,
      )
      ()
    }

    switch action {
    | Play if get().playState !== Playing => startPlaying(get().frame)
    | Seek(newFrame) => {
        AnimationRuntime.AudioRuntime.stop()
        startPlaying(newFrame)
        recordFrame(newFrame)
      }
    | NewFrame(newFrame) if mod(newFrame, Wasm.videoMeta.fps) === 0 => recordFrame(newFrame)
    | Pause => AnimationRuntime.AudioRuntime.stop()
    | SetVolume(volume) => {
        AnimationRuntime.AudioRuntime.setVolume(volume)
        Dom.Storage.localStorage |> Dom.Storage.setItem(volume_key, volume->Js.Int.toString)
      }
    | SetMagnet if get().magnet !== Some(get().frame) =>
      Dom.Storage.localStorage |> Dom.Storage.setItem(
        get_magnet_key(Wasm.videoMeta),
        get().frame->Js.Int.toString,
      )
    | SetMagnet if get().magnet === Some(get().frame) =>
      Dom.Storage.localStorage |> Dom.Storage.removeItem(get_magnet_key(Wasm.videoMeta))
    | _ => ()
    }
  }

  let rec dispatch = action => {
    sideEffect(action, dispatch)
    reducer(action)->set
  }
}
