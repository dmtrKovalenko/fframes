// Monochrome system icons from the ChatGPT design system (@openai/apps-sdk-ui).
// They are sized with `1em` and colored with `currentColor`, so set the size through the font size
// or width/height classes and the color through the text color.

module PlayIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "PlayTriangle"
}

module PauseIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "PauseSm"
}

module RewindIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "ArrowRotateCcw"
}

module ForwardIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "ArrowRotateCw"
}

module VolumeIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "SoundOnReadOutLoudSpeaker"
}

module MuteIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "SoundOffSpeaker"
}

module ExpandIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "ExpandLg"
}

module CollapseIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "CollapseLg"
}

module ChevronDownIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "ChevronDown"
}

module ChevronUpIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "ChevronUp"
}

module LockIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "Lock"
}

module PinIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "Pin"
}

module MusicIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "Music"
}

module CaptionsIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "CaptionOn"
}

module FontIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "Text"
}

module ListViewIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "VideoList"
}

module GridViewIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "VideoGrid"
}

module PlusIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "Plus"
}

module MinusIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "Minus"
}

module ImageIcon = {
  @react.component @module("@openai/apps-sdk-ui/components/Icon")
  external make: (~className: string=?, @as("aria-hidden") ~ariaHidden: bool=?) => React.element =
    "ImageSquare"
}
