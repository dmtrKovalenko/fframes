type namedRange = {
  name: string,
  start: int,
  end: int,
}

@genType
type mediaImport = string

type mediaFolder = Js.Dict.t<mediaImport>

@gentype.as("EditorOptions")
type options = {
  staticMediaFolder: option<mediaFolder>,
  dynamicMediaFolder: option<mediaFolder>,
  ignoreMediaRegex: option<Js.RegExp.t>,
  hideDock: bool,
  loop: bool,
  lockFps: option<int>,
  mediaListLayout: [#grid | #list | #fromAspectRatio],
  rewindStepInSeconds: int,
  dynamicImageLengthLimit: int,
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

@genType
type staticFont = {data: Js.Uint8Array.t, info: fontInfo, name: string}

@genType
type staticAudio = {
  fltp_data: Js.Float32Array.t,
  sample_rate: int,
  name: string,
}

@genType.as("WasmController")
type t = {
  add_audio_source: (string, int, Js.Float32Array.t) => unit,
  add_image_source: (string, string, Js.Undefined.t<string>) => unit,
  add_subtitles_source: (string, string) => int,
  default: unit => Js.Promise.t<initOut>,
  prepare: Js.Undefined.t<int> => Js.Promise.t<videoMeta>,
  render_frame: Js.BigInt.t => string,
  render_preview_frame: Js.BigInt.t => string,
  ingest_font: Js.Uint8Array.t => fontInfo,
  populate_static_fonts_db_with_static_fonts: unit => unit,
  get_static_font_data_by_index: int => Js.Nullable.t<staticFont>,
  get_static_audio_data_by_index: int => Js.Nullable.t<staticAudio>,
}

module type WasmBridge = {
  let videoMeta: videoMeta
  let controller: t
  let options: options
}

external getFrame: (t, Js.BigInt.t) => string = "getFrame"
