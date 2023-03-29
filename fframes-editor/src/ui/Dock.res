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

@send external focus: Dom.Element.t => unit = "focus"

module DockButton = {
  @react.component
  let make = React.memo((~children, ~label, ~onClick: 'a => unit, ~highlight=false) => {
    <button
      onClick={_ => onClick()}
      className={cx([
        DockSpace.baseClass,
        "group hover:scale-110",
        highlight
          ? "bg-gradient-to-tr from-indigo-400/80 to-pink-400/80 hover:from-indigo-300/80 hover:to-pink-300/80"
          : "bg-slate-700 hover:bg-slate-500",
      ])}>
      <span className="sr-only"> {React.string(label)} </span>
      <span className="group-active:scale-90 transition-transform"> {children} </span>
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
  let (isCollapsed, collapsedToggle) = Hooks.useToggle(false)

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
    player.volume->Option.forEach(volume =>
      (volume + 20)->Player.validateVolume->SetVolume->dispatch
    )
  })

  let decreaseVolume = Hooks.useEvent(() => {
    player.volume->Option.forEach(volume =>
      (volume - 20)->Player.validateVolume->SetVolume->dispatch
    )
  })

  let handleSeekLeft = Hooks.useEvent(() => {
    dispatch(Seek(player.frame - 2 * context.videoMeta.fps))
  })

  let handleSeekRight = Hooks.useEvent(() => {
    dispatch(Seek(player.frame + 2 * context.videoMeta.fps))
  })

  let toggleMute = Hooks.useEvent(() => {
    dispatch(SetVolume(0))
  })

  let setMagnet = Hooks.useEvent(() => {
    dispatch(SetMagnet)
  })

  let seekToStart = Hooks.useEvent(() => {
    dispatch(Seek(player.magnet->Utils.Option.unwrapOr(0)))
  })

  let toggleDock = Hooks.useEvent(() => {
    collapsedToggle.toggle()
    Js.Console.log("Press t to show/hide dock controls")
  })

  React.useEffect1(() => {
    let handleKeydown = e => {
      open! Dom

      if (
        e
        ->KeyboardEvent.target
        ->EventTarget.unsafeAsElement
        ->Web.Element.isFocusable
        ->Utils.Bool.invert
      ) {
        switch e->KeyboardEvent.key {
        | " " => handlePlayOrPause()
        | "0" | "H" => seekToStart()
        | "ArrowLeft" if e->KeyboardEvent.shiftKey => seekToStart()
        | "ArrowDown" | "h" if e->KeyboardEvent.ctrlKey => toggleMute()
        | "ArrowLeft" | "j" => handleSeekLeft()
        | "ArrowRight" | "k" => handleSeekRight()
        | "ArrowUp" | "l" => increaseVolume()
        | "ArrowDown" | "h" => decreaseVolume()
        | "m" | "M" => setMagnet()
        | "t" | "T" => collapsedToggle.toggle()
        | "f" | "F" => toggleDock()
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
    className={Cx.cx([
      "absolute bottom-0 w-auto transition-transform transform-gpu left-1/2 px-4 pt-1 space-x-2 bg-slate-900/50 border-t border-x border-gray-100/20 shadow-xl rounded-t-lg backdrop-blur flex -translate-x-1/2",
      isCollapsed ? "translate-y-16 duration-300" : "",
    ])}>
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
      <PlayBackIcon text="2" backward=true className="h-6 w-6" />
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
      <PlayBackIcon text="2" className="h-6 w-6" />
    </DockButton>
    <DockSpace>
      {switch player.volume {
      | Some(volume) => <VolumeIcon high={volume > 50} mute={volume === 0} className="h-6 w-6" />
      | _ => <VolumeMuteIcon className="h-6 w-6 text-gray-500" />
      }}
      <Slider
        disabled={player.volume->Option.isNone}
        min=Player.min_volume
        max=Player.max_volume
        step=1
        value={player.volume->Utils.Option.unwrapOr(0)}
        onValueChange={handleSetVolume}
      />
    </DockSpace>
    <DockDivider />
    <DockButton onClick=setMagnet label="Magnet to this position">
      <MagnetIcon className="h-6 w-6" />
    </DockButton>
    <DockButton onClick=fullScreenToggler.toggle label="Turn on/off full-screen mode">
      <FullScreenIcon className="h-6 w-6" />
    </DockButton>
    <DockButton onClick=toggleDock label="Show/Hide dock controls">
      <CollapseIcon
        className={Cx.cx(["h-6 w-6 transition-transform", isCollapsed ? "rotate-180" : ""])}
      />
    </DockButton>
  </div>
}
