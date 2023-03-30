module RadixTooltip = {
  module Provider = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (~children: React.element) => React.element = "Provider"
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

  module Arrow = {
    @react.component @module("@radix-ui/react-tooltip")
    external make: (~className: string=?) => React.element = "Arrow"
  }
}

@react.component
let make = (~children: React.element, ~content, ~asChild) => {
  <RadixTooltip.Provider>
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>
        {children}
        // {if asChild {
        // } else {
        //   <button type_="button" className="inline-block"> {children} </button>
        // }}
      </RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          className="data-[state=delayed-open]:data-[side=top]:animate-slideDownAndFade data-[state=delayed-open]:data-[side=right]:animate-slideLeftAndFade data-[state=delayed-open]:data-[side=left]:animate-slideRightAndFade data-[state=delayed-open]:data-[side=bottom]:animate-slideUpAndFade select-none rounded-[4px] bg-white text-black px-[15px] py-[10px] text-[15px] leading-none shadow-[hsl(206_22%_7%_/_35%)_0px_10px_38px_-10px,_hsl(206_22%_7%_/_20%)_0px_10px_20px_-15px] will-change-[transform,opacity]"
          sideOffset={5}>
          {content} <RadixTooltip.Arrow className="fill-white" />
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  </RadixTooltip.Provider>
}
