open Cx

// Circular loading indicator drawn in the current text color
@react.component
let make = Utils.neverRerender((~className) => {
  <svg
    className={cx(["animate-spin", className])}
    xmlns="http://www.w3.org/2000/svg"
    fill="none"
    viewBox="0 0 24 24">
    <circle className="opacity-25" cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="2.5" />
    <path stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" d="M21 12a9 9 0 0 0-9-9" />
  </svg>
})
