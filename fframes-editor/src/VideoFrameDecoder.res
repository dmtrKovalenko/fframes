type t

@module("./services/VideoFrameDecoder") @new
external make: unit => t = "VideoFrameDecoder"

@send
external initialize: (t, string) => Js.Promise.t<unit> = "initialize"

@send
external decodeFrameAtTime: (t, float) => Js.Promise.t<Js.Nullable.t<string>> = "decodeFrameAtTime"

@send
external getWidth: t => int = "getWidth"

@send
external getHeight: t => int = "getHeight"

@send
external isAvailable: t => bool = "isAvailable"

@send
external isFailed: t => bool = "isFailed"

@send
external dispose: t => unit = "dispose"
