open Belt
module Promise = Js.Promise

type audioInfo = {
  duration: float,
  sampleRate: int,
  arrayBuffer: Js.ArrayBuffer.t,
  audioData: WebAudio.AudioBuffer.t,
  monoPcmData: Js.Int16Array.t,
}

type imageInfo = {
  src: string,
  width: int,
  height: int,
}

type fontInfo = {
  name: string,
  style: string,
  weight: int,
  unicodeRange: string,
}

@genType
type processedMedia =
  | Font(fontInfo)
  | Subtitles(int)
  | Image(imageInfo)
  | Audio(audioInfo)

type loadableMedia = Loading(string) | Media(processedMedia) | Error(string)

@genType
type mediaImport = string

type action =
  | InitMediaProcessing(Js.Dict.t<mediaImport>)
  | MediaItemProcessed(string, processedMedia)
  | MediaProcessingFinished

module ObserverState = {
  type state = {
    allMediaLoaded: bool,
    mediaList: Belt.Map.String.t<loadableMedia>,
  }

  type action = action

  let initial = {
    allMediaLoaded: false,
    mediaList: Belt.Map.String.empty,
  }

  let reducer = (state, action) => {
    switch action {
    | InitMediaProcessing(imports) => {
        ...state,
        mediaList: imports
        ->Js.Dict.keysToArray
        ->Array.map(relativePath => (Utils.Path.getFilename(relativePath), Loading(relativePath)))
        ->Map.String.fromArray,
      }
    | MediaItemProcessed(name, media) => {
        ...state,
        mediaList: state.mediaList->Map.String.update(name, _ => Some(Media(media))),
      }
    | MediaProcessingFinished => {
        ...state,
        allMediaLoaded: true,
      }
    }
  }
}

module MediaLoaderObserver = UseObservable.MakeObserver(ObserverState)

// This type forces typescript implementation to correctly call the `resolveMedia` and convert values to the rescript world
type forceTsReturnResolveMedia = MediaResolved

@genType.as("MediaResolver")
type mediaResolveFn = (string, string, WasmController.t) => Js.Promise.t<forceTsReturnResolveMedia>

@genType.as("MediaResolverWithOptions")
type mediaResolveFnWithOptions = (
  WasmController.options,
  string,
  string,
  WasmController.t,
) => Js.Promise.t<forceTsReturnResolveMedia>

@module("./MediaResolvers") external resolveAudio: mediaResolveFn = "resolveAudio"
@module("./MediaResolvers") external resolveSubtitles: mediaResolveFn = "resolveSubtitles"
@module("./MediaResolvers") external resolveFont: mediaResolveFn = "resolveFont"
@module("./MediaResolvers") external resolveImage: mediaResolveFnWithOptions = "resolveImage"

// This is pretty dumb of how genType works for typescript.
// It only maps public types to the internal types when using public API, so we can't do this on Promise.then step
@genType
let resolveMedia = (name, media) => {
  MediaLoaderObserver.dispatch(MediaItemProcessed(name, media))

  MediaResolved
}

@genType
let processImports = (
  ~imports: Js.Dict.t<mediaImport>,
  ~wasmController: WasmController.t,
  ~options: WasmController.options,
) => {
  MediaLoaderObserver.dispatch(InitMediaProcessing(imports))
  imports
  ->Js.Dict.toArray
  ->Array.keepMap(((moduleRelativePath, moduleVal)) => {
    let name = moduleRelativePath->Utils.Path.getFilename

    if (
      options.ignoreMediaRegex
      ->Belt.Option.map(regex => Js.RegExp.test(regex, moduleRelativePath))
      ->Utils.Option.unwrapOr(false)
    ) {
      None
    } else {
      switch name {
      | name if name->Js.String.endsWith(".mp3") => Some(resolveAudio)
      | name if name->Js.String.endsWith(".vtt") => Some(resolveSubtitles)
      | name if name->Js.String.endsWith(".ttf") || name->Js.String.endsWith(".otf") =>
        Some(resolveFont)
      | name
        if name->Js.String.endsWith(".png") ||
        name->Js.String.endsWith(".jpg") ||
        name->Js.String.endsWith(".jpeg") =>
        Some(resolveImage(options))
      | _ => None
      }->Option.map(resolveFn => resolveFn(name, moduleVal, wasmController))
    }
  })
  ->Promise.all
  ->Promise.thenResolve(_ => MediaLoaderObserver.dispatch(MediaProcessingFinished))
}
