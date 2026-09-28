open Icons
open Cx
open Webapi
open Belt
module DocumentEvent = Dom.EventTarget.Impl(Dom.Window)

module DockDivider = {
  @react.component
  let make = Utils.neverRerender(() =>
    <div role="separator" className="mx-1 h-5 w-px shrink-0 bg-[var(--color-border)]" />
  )
}

@send external focus: Dom.Element.t => unit = "focus"

type fpsMarker = Good | Slow | Bad | Unknown

let getFpsMarker = (fps, desiredFps) => {
  let desiredFps = desiredFps->Js.Float.fromInt

  switch fps {
  | None => Unknown
  | Some(fps) if fps < desiredFps *. 0.5 => Bad
  | Some(fps) if fps < desiredFps *. 0.8 => Slow
  | _ => Good
  }
}

type dir = Back | Forth

@react.component
let make = (
  ~isFullScreen: bool,
  ~fullScreenToggler: Hooks.toggle,
  ~timelineSize: option<UseEditorLayout.sectionSize>=?,
) => {
  let context = EditorContext.useEditorContext()
  let (player, dispatch) = context.usePlayer()
  let (isCollapsed, collapsedToggle) = Hooks.useToggle(context.options.hideDock)

  // Only use viewport follow if timeline size is available
  let followFrameToViewport = switch timelineSize {
  | Some(size) => Some(UseViewportFollow.useViewportFollow(size))
  | None => None
  }

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
      (volume + context.options.volumeStepFrom0To100)->Player.validateVolume->SetVolume->dispatch
    )
  })

  let decreaseVolume = Hooks.useEvent(() => {
    player.volume->Option.forEach(volume =>
      (volume - context.options.volumeStepFrom0To100)->Player.validateVolume->SetVolume->dispatch
    )
  })

  let handleSeekLeft = Hooks.useEvent(() => {
    let targetFrame = player.frame - context.options.rewindStepInSeconds * context.videoMeta.fps
    dispatch(Seek(targetFrame))
    followFrameToViewport->Belt.Option.forEach(follow => follow(targetFrame))
  })

  let handleSeekRight = Hooks.useEvent(() => {
    let targetFrame = player.frame + context.options.rewindStepInSeconds * context.videoMeta.fps
    dispatch(Seek(targetFrame))
    followFrameToViewport->Belt.Option.forEach(follow => follow(targetFrame))
  })

  let volumeBeforeMute = React.useRef(60)
  let toggleMute = Hooks.useEvent(() => {
    player.volume->Option.forEach(volume =>
      if volume > 0 {
        volumeBeforeMute.current = volume
        dispatch(SetVolume(0))
      } else {
        dispatch(SetVolume(volumeBeforeMute.current))
      }
    )
  })

  let loggedMagnetRef = React.useRef(false)

  let setMagnet = Hooks.useEvent(() => {
    if !loggedMagnetRef.current {
      Js.Console.log("Press 0 or Shift+j to seek to magnet point.")
      loggedMagnetRef.current = true
    }

    dispatch(SetMagnet)
  })

  let seekToStart = Hooks.useEvent(() => {
    let targetFrame = player.magnet->Utils.Option.unwrapOr(0)
    dispatch(Seek(targetFrame))
    followFrameToViewport->Belt.Option.forEach(follow => follow(targetFrame))
  })

  let seekToEnd = Hooks.useEvent(() => {
    let targetFrame = context.videoMeta.durationInFrames
    dispatch(Seek(targetFrame))
    followFrameToViewport->Belt.Option.forEach(follow => follow(targetFrame))
  })

  let switchScene = Hooks.useEvent(dir => {
    context.videoMeta.scenesTimeline
    ->Js.Nullable.toOption
    ->Option.flatMap(timeline => {
      let nextSceneIndex = switch dir {
      | Back => timeline->Js.Array.findIndex(scene => scene.end >= player.frame)
      | Forth => timeline->Js.Array.findIndex(scene => scene.start > player.frame)
      }

      timeline[nextSceneIndex]
    })
    ->Option.forEach(scene => {
      dispatch(Seek(scene.start))
      followFrameToViewport->Belt.Option.forEach(follow => follow(scene.start))
    })
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
        | "0" | "J" | "Home" => seekToStart()
        | "ArrowLeft" if e->KeyboardEvent.shiftKey => seekToStart()
        | "G" | "End" => seekToEnd()
        | "ArrowRight" if e->KeyboardEvent.shiftKey => seekToEnd()
        | "ArrowDown" | "h" if e->KeyboardEvent.ctrlKey => toggleMute()
        | "ArrowLeft" | "j" => handleSeekLeft()
        | "ArrowRight" | "k" => handleSeekRight()
        | "ArrowUp" | "l" => increaseVolume()
        | "ArrowDown" | "h" => decreaseVolume()
        | "m" | "M" => setMagnet()
        | "t" | "T" => collapsedToggle.toggle()
        | "f" | "F" => fullScreenToggler.toggle()
        | "s" | "w" => switchScene(Forth)
        | "S" | "b" => switchScene(Back)
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

  let rewindStep = context.options.rewindStepInSeconds->Int.toString
  let fps = switch debouncedFps {
  | Some(fps) =>
    fps
    ->Js.Math.min(context.videoMeta.fps->Js.Float.fromInt)
    ->Js.Float.toFixedWithPrecision(~digits=0)
  | None => context.videoMeta.fps->Int.toString
  }

  {
    if isCollapsed {
      <div className="absolute bottom-4 left-1/2 -translate-x-1/2">
        <div
          className="rounded-full bg-surface-elevated shadow-[var(--shadow-300),var(--shadow-hairline)]">
          <IconButton onClick=toggleDock label="Show controls" shortcut="T">
            <ChevronUpIcon />
          </IconButton>
        </div>
      </div>
    } else {
      <div
        role="toolbar"
        ariaLabel="Playback controls"
        className="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-1 whitespace-nowrap rounded-full bg-surface-elevated p-1.5 text-default shadow-[var(--shadow-300),var(--shadow-hairline)]">
        <div className="flex items-baseline gap-1 px-3 text-sm tabular">
          <span className="font-medium">
            {player.frame->Utils.Duration.formatFrame(context.videoMeta.fps)->React.string}
          </span>
          <span className="text-tertiary"> {React.string("/")} </span>
          <span className="text-secondary">
            {context.videoMeta.durationInFrames
            ->Utils.Duration.formatFrame(context.videoMeta.fps)
            ->React.string}
          </span>
        </div>
        <Tooltip
          content={switch context.videoMeta.originalFps {
          | Some(originalFps) =>
            React.string(
              `Preview locked to ${context.videoMeta.fps->Int.toString} fps, the video renders at ${originalFps->Int.toString} fps`,
            )
          | None => React.string("Preview frame rate")
          }}>
          <div
            tabIndex=0
            className="flex h-9 items-center gap-1.5 rounded-full px-2.5 text-xs text-secondary tabular outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-ring)]">
            <span
              ariaHidden=true
              className={cx([
                "size-1.5 rounded-full transition-colors duration-[400ms]",
                switch getFpsMarker(debouncedFps, context.videoMeta.fps) {
                | Good => "bg-green-400"
                | Slow => "bg-orange-400"
                | Bad => "bg-red-400"
                | Unknown => "bg-gray-400"
                },
              ])}
            />
            <span className="inline-block w-[2ch] text-right"> {React.string(fps)} </span>
            <span> {React.string("fps")} </span>
            {switch context.videoMeta.originalFps {
            | Some(_) => <LockIcon className="size-3.5" />
            | None => React.null
            }}
          </div>
        </Tooltip>
        <DockDivider />
        <IconButton onClick=handleSeekLeft label={`Back ${rewindStep}s`} shortcut={`←`}>
          <RewindIcon />
        </IconButton>
        <IconButton
          onClick=handlePlayOrPause
          variant=IconButton.Brand
          size=IconButton.Lg
          label={switch player.playState {
          | Playing => "Pause"
          | CantPlay => "Loading media"
          | _ => "Play"
          }}
          shortcut="Space">
          {switch player.playState {
          | CantPlay => <Spinner className="size-5" />
          | Playing => <PauseIcon />
          | Paused
          | WaitingForAction =>
            <PlayIcon />
          }}
        </IconButton>
        <IconButton onClick=handleSeekRight label={`Forward ${rewindStep}s`} shortcut={`→`}>
          <ForwardIcon />
        </IconButton>
        <DockDivider />
        <div className="flex items-center gap-1 pr-3">
          {switch player.volume {
          | Some(volume) =>
            <IconButton
              onClick=toggleMute
              label={volume === 0 ? "Unmute" : "Mute"}
              pressed={volume === 0}
              shortcut={`Ctrl ↓`}>
              {volume === 0 ? <MuteIcon /> : <VolumeIcon />}
            </IconButton>
          | None =>
            <Tooltip content={React.string("This video has no audio")}>
              <span
                tabIndex=0
                className="inline-flex size-9 items-center justify-center text-disabled outline-none [&>svg]:size-5">
                <MuteIcon />
              </span>
            </Tooltip>
          }}
          <Slider
            label="Volume"
            disabled={player.volume->Option.isNone}
            min=Player.min_volume
            max=Player.max_volume
            step=1
            value={player.volume->Utils.Option.unwrapOr(0)}
            onValueChange={handleSetVolume}
          />
        </div>
        <DockDivider />
        <IconButton
          onClick=setMagnet
          label={switch player.magnet {
          | Some(frame) if frame === player.frame => "Remove start pin"
          | Some(_) => "Move start pin here"
          | None => "Pin start here"
          }}
          pressed={player.magnet->Option.isSome}
          shortcut="M">
          <PinIcon />
        </IconButton>
        <ZoomControls />
        <IconButton
          onClick=fullScreenToggler.toggle
          label={isFullScreen ? "Exit full screen" : "Full screen"}
          shortcut="F">
          {isFullScreen ? <CollapseIcon /> : <ExpandIcon />}
        </IconButton>
        <DockDivider />
        <IconButton onClick=toggleDock label="Hide controls" shortcut="T">
          <ChevronDownIcon />
        </IconButton>
      </div>
    }
  }
}
