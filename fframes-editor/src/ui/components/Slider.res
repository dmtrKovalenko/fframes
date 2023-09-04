module RadixSlider = {
  module Root = {
    @react.component @module("@radix-ui/react-slider")
    external make: (
      ~value: array<int>,
      ~onValueChange: array<int> => unit=?,
      ~step: int,
      ~min: int,
      ~max: int,
      ~disabled: bool=?,
      ~children: React.element,
      ~className: string=?,
    ) => React.element = "Root"
  }

  module Track = {
    @react.component @module("@radix-ui/react-slider")
    external make: (~children: React.element, ~className: string=?) => React.element = "Track"
  }

  module Range = {
    @react.component @module("@radix-ui/react-slider")
    external make: (~className: string=?) => React.element = "Range"
  }

  module Thumb = {
    @react.component @module("@radix-ui/react-slider")
    external make: (~className: string=?) => React.element = "Thumb"
  }
}

/**
 *  Reusable slider component used for the volume slider and timeline scaling selector
 */
@react.component
let make = (~onValueChange, ~disabled, ~value, ~min, ~max, ~step) => {
  let handleChange = React.useCallback1(newValue => {
    newValue[0]->onValueChange
  }, [onValueChange])

  <RadixSlider.Root
    step
    min
    max
    disabled
    value=[value]
    onValueChange=handleChange
    className="relative flex items-center select-none w-28 h-4 mx-2">
    <RadixSlider.Track className="relative flex-grow h-1 rounded-full bg-slate-800">
      <RadixSlider.Range className="absolute bg-gray-100 rounded-full h-full" />
    </RadixSlider.Track>
    <RadixSlider.Thumb
      className="block cursor-grab w-[13px] h-[13px] bg-white transition-transform shadow-xl rounded-full focus:bg-gradient-to-tr from-indigo-400 to-pink-400"
    />
  </RadixSlider.Root>
}
