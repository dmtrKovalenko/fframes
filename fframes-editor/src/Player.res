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
  volume: option<float>,
}

@genType
type action =
  | Seek(int)
  | NewFrame(int)
  | AllowPlay
  | Play
  | Pause
  | SetVolume(float)

let currentFps: ref<option<int>> = ref(None)

@inline
let volume_key = "ffvolume"
@inline
let frame_key = "fframe"

let min_volume = 0.
let max_volume = 1.

module MakePlayer = (Wasm: WasmController.WasmBridge) => {
  module PlayerState = {
    type t = state

    let previousSavedFrame =
      Dom.Storage.getItem(frame_key, Dom.Storage.localStorage)
      ->Option.map(Js.Int.fromString)
      ->Utils.Option.flatten
      ->Utils.Option.unwrapOr(0)

    let volume = switch Dom.Storage.getItem(volume_key, Dom.Storage.localStorage) {
    | Some(savedValue) if Wasm.videoMeta.hasAudio => Some(savedValue->Js.Float.fromString)
    | None if Wasm.videoMeta.hasAudio => Some(0.6)
    | _ => None
    }

    let initial = switch MediaLoader.MediaLoaderObserver.get() {
    | state if state.allMediaLoaded => {
        frame: previousSavedFrame,
        startPlayingFrame: previousSavedFrame,
        playState: WaitingForAction,
        fpsLimit: Some(Wasm.videoMeta.fps),
        volume: volume,
        svg: Wasm.controller.render_frame(0->Js.BigInt.fromInt)->Utils.Option.some,
      }
    | _ => {
        frame: previousSavedFrame,
        startPlayingFrame: previousSavedFrame,
        playState: CantPlay,
        svg: None,
        volume: volume,
        fpsLimit: Some(Wasm.videoMeta.fps),
      }
    }
  }

  include UseObservable.Pubsub(PlayerState)

  let reducer = action => {
    let state = get()
    switch action {
    | Seek(frame) | NewFrame(frame) if frame > Wasm.videoMeta.durationInFrames || frame < 0 => {
        let frame = 0
        let svg = Wasm.controller.render_frame(frame->Js.BigInt.fromInt)

        {...state, frame: frame, svg: Some(svg), playState: Paused, startPlayingFrame: 0}
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
        frame: 0,
        playState: Playing,
      }
    | Play => {...state, playState: Playing, startPlayingFrame: state.frame}
    | Pause => {...state, playState: Paused}
    | SetVolume(volume) => {
        ...state,
        volume: switch volume {
        | volume if volume > max_volume => Some(max_volume)
        | volume if volume < min_volume => Some(min_volume)
        | _ => Some(volume)
        },
      }
    }
  }

  let sideEffect = (action, dispatch) => {
    let startPlaying = currentFrame => {
      let onFrame = (~secondsFromStart) => {
        let nextFrame =
          (secondsFromStart *. Wasm.videoMeta.fps->Float.fromInt +.
            get().startPlayingFrame->Float.fromInt)->Utils.Math.floor

        if nextFrame !== get().frame {
          dispatch(NewFrame(nextFrame))
        }

        get().playState === Playing
      }

      get().volume->Option.map(AnimationRuntime.AudioRuntime.setVolume)->ignore
      AnimationRuntime.AudioRuntime.startAnimation(
        ~onFrame,
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

        Dom.Storage.localStorage |> Dom.Storage.setItem(frame_key, newFrame->Js.Int.toString)
      }
    | NewFrame(newFrame) if mod(newFrame, Wasm.videoMeta.fps) === 0 =>
      Dom.Storage.localStorage |> Dom.Storage.setItem(frame_key, newFrame->Js.Int.toString)
    | Pause => AnimationRuntime.AudioRuntime.stop()
    | SetVolume(value) => {
        AnimationRuntime.AudioRuntime.setVolume(value)
        Dom.Storage.localStorage |> Dom.Storage.setItem(volume_key, value->Js.Float.toString)
      }
    | _ => ()
    }
  }

  let rec dispatch = action => {
    sideEffect(action, dispatch)
    reducer(action)->set
  }
}
