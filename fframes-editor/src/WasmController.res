type namedRange = {
  name: string,
  start: int,
  end: int,
}

@gentype.as("EditorOptions")
type options = {
  ignoreMediaRegex: option<Js.RegExp.t>,
  hideDock: bool,
  loop: bool,
  lockFps: option<int>,
  mediaListLayout: [#grid | #list | #fromAspectRatio],
  rewindStepInSeconds: int,
  imageLengthLimit: int,
  volumeStepFrom0To100: int,
}

@genType.as("VideoMeta")
type videoMeta = {
  name: string,
  width: int,
  height: int,
  fps: int,
  durationInFrames: int,
  hasAudio: bool,
  originalFps: option<int>,
  audioMap: Js.Nullable.t<array<namedRange>>,
  scenesTimeline: Js.Nullable.t<array<namedRange>>,
}

type fontInfo = {
  name: Js.Nullable.t<Js.Uint8Array.t>,
  weight: int,
  style: string,
}

type initOut = {prepare: (int, int) => int}

@genType.as("WasmController")
type t = {
  add_audio_source: (string, ReScriptJs.Js.Int16Array.t) => unit,
  add_image_source: (string, string, Js.Undefined.t<string>) => unit,
  add_subtitles_source: (string, string) => int,
  default: unit => Js.Promise.t<initOut>,
  prepare: Js.Undefined.t<int> => Js.Promise.t<videoMeta>,
  render_frame: Js.BigInt.t => string,
  render_preview_frame: Js.BigInt.t => string,
  ingest_font: Js.Uint8Array.t => fontInfo,
}

module type WasmBridge = {
  let videoMeta: videoMeta
  let controller: t
  let options: options
}

external getFrame: (t, Js.BigInt.t) => string = "getFrame"
