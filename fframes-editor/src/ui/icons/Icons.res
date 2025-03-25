// Unfortunately @react.component requires type to be inlined
module MusicalNotesIcon = {
  @react.component @module("./MusicalNoteIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "MusicalNoteIcon"
}
module FontIcon = {
  @react.component @module("./FontIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "FontIcon"
}
module CaptionsIcon = {
  @react.component @module("./CaptionsIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "CaptionsIcon"
}
module PlayIcon = {
  @react.component @module("./PlayIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "PlayIcon"
}
module PlayBackIcon = {
  @react.component @module("./PlayBackIcon")
  external make: (
    ~text: string,
    ~backward: bool=?,
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "PlayBackIcon"
}
module VolumeIcon = {
  @react.component @module("./VolumeIcon")
  external make: (
    ~high: bool=?,
    ~mute: bool=?,
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "VolumeIcon"
}
module MagnetIcon = {
  @react.component @module("./MagnetIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "MagnetIcon"
}
module FullScreenIcon = {
  @react.component @module("./FullScreenIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "FullScreenIcon"
}
module CollapseIcon = {
  @react.component @module("./CollapseIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "CollapseIcon"
}
module PauseIcon = {
  @react.component @module("./PauseIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "PauseIcon"
}
module GridViewIcon = {
  @react.component @module("./GridViewIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "GridViewIcon"
}
module ListViewIcon = {
  @react.component @module("./ListViewIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "ListViewIcon"
}
module LockIcon = {
  @react.component @module("./LockIcon")
  external make: (
    ~color: string=?,
    ~className: string=?,
    ~style: ReactDOM.Style.t=?,
  ) => React.element = "LockIcon"
}

@module("./magnet.svg") @val
external magnetRawIcon: string = "default"
