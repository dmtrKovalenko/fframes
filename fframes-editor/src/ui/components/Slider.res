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
    external make: (~className: string=?, @as("aria-label") ~ariaLabel: string=?) => React.element =
      "Thumb"
  }
}

/**
 *  Reusable slider component used for the volume slider, styled as the system slider
 */
@react.component
let make = (~onValueChange, ~disabled, ~value, ~min, ~max, ~step, ~label) => {
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
    className="group relative flex h-4 w-24 touch-none select-none items-center data-[disabled]:opacity-50">
    <RadixSlider.Track className="relative h-1 grow rounded-2xs bg-primary-soft-alpha">
      <RadixSlider.Range className="absolute h-full rounded-2xs bg-[var(--color-text)]" />
    </RadixSlider.Track>
    <RadixSlider.Thumb
      ariaLabel=label
      className="block size-3.5 cursor-grab rounded-full bg-gray-0 shadow-[inset_0_0_0_2px_var(--color-text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-ring)] active:cursor-grabbing"
    />
  </RadixSlider.Root>
}
