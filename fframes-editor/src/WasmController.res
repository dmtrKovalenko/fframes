type namedRange = {
  name: string,
  start: int,
  end: int,
}

@genType.as("VideoMeta")
type videoMeta = {
  name: string,
  width: int,
  height: int,
  fps: int,
  durationInFrames: int,
  hasAudio: bool,
  audioMap: Js.Nullable.t<array<namedRange>>,
  scenesTimeline: Js.Nullable.t<array<namedRange>>,
}

type fontInfo = {
  name: Js.Nullable.t<Js.Uint8Array.t>,
  weight: int,
  style: string,
}

type initOut = {prepare: unit => int}

@genType.as("WasmController")
type t = {
  add_audio_source: (string, ReScriptJs.Js.Int16Array.t) => unit,
  add_image_source: (string, string, Js.Nullable.t<string>) => unit,
  add_subtitles_source: (string, string) => int,
  default: unit => Js.Promise.t<initOut>,
  prepare: unit => Js.Promise.t<videoMeta>,
  render_frame: Js.BigInt.t => string,
  render_preview_frame: Js.BigInt.t => string,
  ingest_font: Js.Uint8Array.t => fontInfo,
}

module type WasmBridge = {
  let videoMeta: videoMeta
  let controller: t
}

external getFrame: (t, Js.BigInt.t) => string = "getFrame"
