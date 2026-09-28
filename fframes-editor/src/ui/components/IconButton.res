open Cx

type variant =
  // Quiet control, the default for everything but the primary action
  | Ghost
  // The single primary action of a surface
  | Solid
  // The primary action in the fframes brand color
  | Brand

type size = Md | Lg

let focusRing = "outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--color-surface)]"

// Round icon-only button with an accessible label and a tooltip naming it (and its shortcut)
@react.component
let make = (
  ~label: string,
  ~onClick: unit => unit,
  ~children,
  ~shortcut: option<string>=?,
  ~variant=Ghost,
  ~size=Md,
  ~pressed: option<bool>=?,
  ~className="",
) => {
  let button =
    <button
      type_="button"
      ariaLabel=label
      onClick={_ => onClick()}
      className={cx([
        "inline-flex shrink-0 items-center justify-center rounded-full transition-colors duration-150 [&>svg]:size-5",
        focusRing,
        switch size {
        | Md => "size-9"
        | Lg => "size-10"
        },
        switch variant {
        | Ghost => "text-secondary hover:bg-primary-ghost-hover hover:text-default active:bg-primary-ghost-active"
        | Solid => "bg-primary-solid text-primary-solid hover:bg-primary-solid-hover active:bg-primary-solid-active"
        | Brand => "bg-brand text-on-brand hover:bg-brand-hover active:bg-brand-active"
        },
        className,
      ])}>
      children
    </button>

  <Tooltip content={React.string(label)} ?shortcut>
    {switch pressed {
    // rescript-react has no aria-pressed prop
    | Some(pressed) => React.cloneElement(button, {"aria-pressed": pressed})
    | None => button
    }}
  </Tooltip>
}
