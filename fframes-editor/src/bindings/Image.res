type t

@new external make: (~width: int, ~height: int) => t = "Image"

@set
external onLoad: (t, unit => unit) => unit = "onload"

@set
external onError: (t, unit => unit) => unit = "onerror"

@set
external setSrc: (t, string) => unit = "src"

@scope("window") @val
external btoa: string => string = "btoa"
