open WebAudio
open Belt

/* @returns whether continue execution or not */
type onFrame = (~secondsFromStart: float) => bool

module AudioRuntime = {
  let rafId: ref<option<Webapi.rafId>> = ref(None)
  let playingSources: ref<array<(WasmController.namedRange, AudioNode.t)>> = ref([])
  let startTime = ref(0.)
  let lastFrameTime = ref(None)
  let runtimeFps = ref(None)
  let smoothing = 0.9
  let ctx = AudioContext.create()
  let gain = ctx->AudioContext.createGain

  let setVolume = value => {
    gain["gain"]["value"] = value->Float.fromInt /. 100.
  }

  let stop = () => {
    rafId.contents->Belt.Option.map(Webapi.cancelAnimationFrame)->ignore
    playingSources.contents->Array.forEach(nameAndSource => {
      let (_, source) = nameAndSource
      source->AudioNode.stop
    })
  }

  let rec frame = (~onFrame: onFrame, _timestamp) => {
    let currentTime = ctx->AudioContext.getCurrentTime
    let thisFrameTime =
      lastFrameTime.contents->Belt.Option.map(lastFrameTime => currentTime -. lastFrameTime)

    let secondsFromStart = currentTime -. startTime.contents
    runtimeFps :=
      switch (runtimeFps.contents, thisFrameTime) {
      | (Some(lastFps), Some(thisFrameTime)) if thisFrameTime > 0. =>
        Some(lastFps *. smoothing +. 1. /. thisFrameTime *. (1. -. smoothing))
      | (_, Some(thisFrameTime)) if thisFrameTime > 0. => Some(1. /. thisFrameTime)
      | _ => None
      }

    if onFrame(~secondsFromStart) {
      lastFrameTime := Some(currentTime)
      rafId := Some(Webapi.requestCancellableAnimationFrame(frame(~onFrame)))
    } else {
      stop()
    }
  }

  let connectAudioFiles = (ctx, videoMeta: WasmController.videoMeta) => {
    let {mediaList} = MediaLoader.MediaLoaderObserver.get()

    videoMeta.audioMap
    ->Js.Nullable.toOption
    ->Utils.Option.unwrapOr([])
    ->Array.keepMap(track => {
      mediaList
      ->Map.String.getExn(track.name)
      ->(
        (media: MediaLoader.loadableMedia) =>
          switch media {
          | Media(media) =>
            switch media {
            | Audio(info) => Some((track, info))
            | _ => None
            }
          | _ => None
          }
      )
    })
    ->Array.map(((track, info)) => {
      let source = ctx->AudioContext.createBufferSource
      source->AudioNode.setBuffer(info.audioData)

      (track, source)
    })
  }

  let startAnimation = (~onFrame, ~currentFrame, ~videoMeta) => {
    stop() // in case if other animation playing

    playingSources := ctx->connectAudioFiles(videoMeta)

    gain->AudioNode.connect(ctx.destination)->ignore
    startTime := ctx.currentTime

    @inline
    let framesToSeconds = frames => frames->Float.fromInt /. videoMeta.fps->Float.fromInt

    playingSources.contents->Array.forEach(((track, source)) => {
      let offset = (currentFrame - track.start)->framesToSeconds
      let duration = (track.end - currentFrame)->framesToSeconds->Js.Math.max(0.)

      source->AudioNode.connect(gain)

      if offset < 0. {
        source->AudioNode.startWithOffset(
          ~startTime=startTime.contents +. Js.Math.abs(offset),
          ~offset=0.,
          ~duration,
        )
      } else {
        source->AudioNode.startWithOffset(~startTime=startTime.contents, ~offset, ~duration)
      }
    })

    Webapi.requestAnimationFrame(frame(~onFrame))
  }
}
