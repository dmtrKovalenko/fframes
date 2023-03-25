open Icons
open Cx
open Webapi
open Belt
module DocumentEvent = Dom.EventTarget.Impl(Dom.Window)

module DockDivider = {
  @react.component
  let make = Utils.neverRerender(() =>
    <div> <hr className="mx-2 h-9 border-gray-600 border bg-none" /> </div>
  )
}

module DockSpace = {
  let baseClass = "flex items-center justify-center p-2 shadow rounded-xl relative bottom-3 bg-slate-700 transiton-all duration-300"

  @react.component
  let make = React.memo((~children, ~className="") => {
    <div className={cx([baseClass, className])}> {children} </div>
  })
}

module DockButton = {
  @react.component
  let make = React.memo((~children, ~label, ~onClick: 'a => unit, ~highlight=false) => {
    <button
      onClick={_ => onClick()}
      className={cx([
        DockSpace.baseClass,
        "hover:scale-110",
        highlight
          ? "bg-gradient-to-tr from-indigo-400/80 to-pink-400/80 hover:from-indigo-300/80 hover:to-pink-300/80"
          : "bg-slate-700 hover:bg-slate-500",
      ])}>
      <span className="sr-only"> {React.string(label)} </span> {children}
    </button>
  })
}

type fpsMarker = Green | Yellow | Red | White

let getFpsMarker = (fps, desiredFps) => {
  let desiredFps = desiredFps->Js.Float.fromInt

  switch fps {
  | None => White
  | Some(fps) if fps < desiredFps *. 0.5 => Red
  | Some(fps) if fps < desiredFps *. 0.8 => Yellow
  | _ => Green
  }
}

@react.component
let make = (~fullScreenToggler: Hooks.toggle) => {
  let context = EditorContext.useEditorContext()
  let (player, dispatch) = context.usePlayer()

  let (debouncedFps, _) = UseDebounce.useThrottle(
    AnimationRuntime.AudioRuntime.runtimeFps.contents,
    ~ms=100,
  )

  let handlePlayOrPause = Hooks.useEvent(_ => {
    switch player.playState {
    | Playing => Pause
    | _ => Play
    }
    ->dispatch
    ->ignore
  })

  let handleSetVolume = Hooks.useEvent(value => {
    dispatch(SetVolume(value))
  })

  let increaseVolume = Hooks.useEvent(() => {
    player.volume->Option.forEach(volume => dispatch(SetVolume(volume +. 0.2)))
  })

  let decreaseVolume = Hooks.useEvent(() => {
    player.volume->Option.forEach(volume => dispatch(SetVolume(volume -. 0.2)))
  })

  let handleSeekLeft = Hooks.useEvent(() => {
    dispatch(Seek(player.frame - 2 * context.videoMeta.fps))
  })

  let handleSeekRight = Hooks.useEvent(() => {
    dispatch(Seek(player.frame + 2 * context.videoMeta.fps))
  })

  let toggleDock = () => {
    ()
  }

  let toggleMute = () => {
    ()
  }

  let setMagnet = () => {
    ()
  }

  React.useEffect1(() => {
    let handleKeydown = e => {
      if (
        e
        ->Dom.KeyboardEvent.target
        ->Dom.EventTarget.unsafeAsElement
        ->Web.Element.isFocusable
        ->Utils.Bool.invert
      ) {
        switch e->Dom.KeyboardEvent.key {
        | " " => handlePlayOrPause()
        | "0" => dispatch(Seek(0))
        | "ArrowLeft" | "h" | "H" if e->Dom.KeyboardEvent.altKey => dispatch(Seek(0))
        | "ArrowLeft" | "h" | "H" => handleSeekLeft()
        | "ArrowRight" | "l" | "L" => handleSeekRight()
        | "ArrowUp" | "k" | "K" => increaseVolume()
        | "ArrowDown" | "j" | "J" => decreaseVolume()
        | "m" | "M" if e->Dom.KeyboardEvent.metaKey => setMagnet()
        | "m" => toggleMute()
        | "t" | "T" => toggleDock()
        | "f" | "F" => fullScreenToggler.toggle()
        | _ => ()
        }
      }
    }

    Dom.window
    |> DocumentEvent.asEventTarget
    |> Dom.EventTarget.addKeyDownEventListener(handleKeydown)

    Some(
      () =>
        Dom.window
        |> DocumentEvent.asEventTarget
        |> Dom.EventTarget.removeKeyDownEventListener(handleKeydown),
    )
  }, [])

  <div
    className="absolute bottom-0 w-auto left-1/2 px-4 pt-1 space-x-2 bg-[#2a3441]/75 border-t border-x border-gray-100/5 shadow-xl rounded-t-lg backdrop-blur flex transform -translate-x-1/2">
    <DockSpace className="tabular-nums space-x-1">
      <span> {player.frame->Utils.Duration.formatFrame(context.videoMeta.fps)->React.string} </span>
      <span className="normal-nums relative bottom-px"> {React.string(" / ")} </span>
      <span>
        {context.videoMeta.durationInFrames
        ->Utils.Duration.formatFrame(context.videoMeta.fps)
        ->React.string}
      </span>
    </DockSpace>
    <DockSpace>
      <span className="mr-2 ml-2"> {React.string("FPS")} </span>
      <span
        className={cx([
          "tabular-nums w-[3ch] font-medium transition-colors duration-[400ms]",
          switch getFpsMarker(debouncedFps, context.videoMeta.fps) {
          | Green => "text-green-500"
          | Yellow => "text-yellow-500"
          | Red => "text-red-500"
          | White => "text-white"
          },
        ])}>
        {switch debouncedFps {
        | Some(fps) =>
          fps
          ->Js.Math.min(context.videoMeta.fps->Js.Float.fromInt)
          ->Js.Float.toFixedWithPrecision(~digits=0)
          ->React.string
        | None => context.videoMeta.fps->Js.Int.toString->React.string
        }}
      </span>
    </DockSpace>
    <DockDivider />
    <DockButton onClick=handleSeekLeft label="Play forward 5 seconds">
      <PlayBackIcon className="h-6 w-6" />
    </DockButton>
    <DockButton onClick=handlePlayOrPause highlight=true label="Play">
      {switch player.playState {
      | CantPlay => <Spinner className="h-6 w-6" />
      | Playing => <PauseIcon className="h-6 w-6" />
      | Paused
      | WaitingForAction =>
        <PlayIcon className="h-6 w-6" />
      }}
    </DockButton>
    <DockButton onClick=handleSeekRight label="Play back 5 seconds">
      <PlayBackIcon className="h-6 w-6 rotate-180" />
    </DockButton>
    <DockSpace>
      {switch player.volume {
      | Some(volume) if volume > 0. => <VolumeIcon className="h-6 w-6" />
      | Some(_) => <VolumeMuteIcon className="h-6 w-6" />
      | _ => <VolumeMuteIcon className="h-6 w-6 text-gray-500" />
      }}
      <Slider
        disabled={player.volume->Option.isNone}
        min=Player.min_volume
        max=Player.max_volume
        step=0.1
        value={player.volume->Utils.Option.unwrapOr(0.0)}
        onValueChange={handleSetVolume}
      />
    </DockSpace>
    <DockDivider />
    <DockButton onClick=Js.Console.log label="Magnet to this position">
      <MagnetIcon className="h-6 w-6" />
    </DockButton>
    <DockButton onClick=fullScreenToggler.toggle label="Full screen">
      <FullScreenIcon className="h-6 w-6" />
    </DockButton>
    <DockButton onClick=Js.Console.log label="Collapse control bar">
      <CollapseIcon className="h-6 w-6" />
    </DockButton>
  </div>
}
