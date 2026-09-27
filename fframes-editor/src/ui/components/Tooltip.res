module RadixTooltip = {
  module Provider = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (~children: React.element, ~delayDuration: int=?) => React.element = "Provider"
  }

  module Root = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (~children: React.element, ~className: string=?) => React.element = "Root"
  }

  module Trigger = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (
      ~children: React.element,
      ~className: string=?,
      ~asChild: bool=?,
    ) => React.element = "Trigger"
  }

  module Portal = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (~children: React.element, ~className: string=?) => React.element = "Portal"
  }

  module Content = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (
      ~children: React.element,
      ~className: string=?,
      ~sideOffset: int=?,
    ) => React.element = "Content"
  }
}

// One provider for the whole editor so moving between controls shows tooltips without the delay
module Provider = {
  @react.component
  let make = (~children) =>
    <RadixTooltip.Provider delayDuration=400> children </RadixTooltip.Provider>
}

// Compact system tooltip: inverted surface, body-small text and an optional keyboard shortcut
@react.component
let make = (~children: React.element, ~content, ~shortcut: option<string>=?, ~asChild=true) => {
  <RadixTooltip.Root>
    <RadixTooltip.Trigger asChild> {children} </RadixTooltip.Trigger>
    <RadixTooltip.Portal>
      <RadixTooltip.Content
        sideOffset={8}
        className="z-50 flex max-w-[300px] select-none items-center gap-2 rounded-md bg-gray-700 px-2 py-0.5 text-sm text-gray-0 shadow-md animate-tooltip-in">
        {content}
        {switch shortcut {
        | Some(shortcut) =>
          <kbd className="font-sans text-xs text-gray-300"> {React.string(shortcut)} </kbd>
        | None => React.null
        }}
      </RadixTooltip.Content>
    </RadixTooltip.Portal>
  </RadixTooltip.Root>
}
