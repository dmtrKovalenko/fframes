// Light/dark follow the operating system, like ChatGPT. Every color here is a value of the
// @openai/apps-sdk-ui tokens so the canvases match the DOM chrome that uses the CSS variables.

type t = Light | Dark

type mediaQueryList
@val @scope("window") external matchMedia: string => mediaQueryList = "matchMedia"
@get external matches: mediaQueryList => bool = "matches"
@send
external addChangeListener: (mediaQueryList, @as("change") _, unit => unit) => unit =
  "addEventListener"
@send
external removeChangeListener: (mediaQueryList, @as("change") _, unit => unit) => unit =
  "removeEventListener"

let applyToDocument: string => unit = %raw(`theme => { document.documentElement.dataset.theme = theme }`)

let darkQuery = "(prefers-color-scheme: dark)"

let resolve = (preference: [#system | #light | #dark]) =>
  switch preference {
  | #light => Light
  | #dark => Dark
  | #system => matchMedia(darkQuery)->matches ? Dark : Light
  }

let toString = theme =>
  switch theme {
  | Light => "light"
  | Dark => "dark"
  }

// Resolves the theme preference, keeps it in sync with the OS and exposes it on <html data-theme>
// so tokens also apply to portals such as tooltips.
let useTheme = (preference: [#system | #light | #dark]) => {
  let (theme, setTheme) = React.useState(() => resolve(preference))

  React.useEffect1(() => {
    setTheme(_ => resolve(preference))

    switch preference {
    | #system => {
        let query = matchMedia(darkQuery)
        let onChange = () => setTheme(_ => resolve(preference))
        query->addChangeListener(onChange)
        Some(() => query->removeChangeListener(onChange))
      }
    | _ => None
    }
  }, [preference])

  React.useLayoutEffect1(() => {
    theme->toString->applyToDocument
    None
  }, [theme])

  theme
}

let context = React.createContext(Dark)
let provider = React.Context.provider(context)

module Provider = {
  @react.component
  let make = (~value: t, ~children) =>
    React.createElement(provider, {"value": value, "children": children})
}

let use = () => React.useContext(context)

type palette = {
  text: string,
  textSecondary: string,
  textTertiary: string,
  border: string,
  softAlpha: string,
  surface: string,
  accent: string,
  // fframes brand orange, only for the playhead
  brand: string,
  // Categorical accents for scenes, the 400 steps of the system hues
  scenes: array<string>,
}

// Same font stack as --font-sans
let fontSans = `ui-sans-serif, -apple-system, system-ui, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif`
// --font-text-xs-size
let canvasFont = `12px ${fontSans}`
let canvasFontMedium = `500 12px ${fontSans}`

// Orange is left out: it is the brand color and marks the playhead
let scenes = ["#0285ff", "#04b84c", "#924ff7", "#ff66ad", "#ffc300", "#fa423e"]

let dark = {
  text: "#ffffff",
  textSecondary: "#afafaf",
  textTertiary: "#8f8f8f",
  border: "rgb(255 255 255 / 0.12)",
  softAlpha: "rgb(255 255 255 / 0.08)",
  surface: "#181818",
  accent: "#339cff",
  brand: "#fb6a22",
  scenes: scenes,
}

let light = {
  text: "#0d0d0d",
  textSecondary: "#5d5d5d",
  textTertiary: "#8f8f8f",
  border: "rgb(13 13 13 / 0.1)",
  softAlpha: "rgb(13 13 13 / 0.05)",
  surface: "#f9f9f9",
  accent: "#0169cc",
  brand: "#fb6a22",
  scenes: scenes,
}

let palette = theme =>
  switch theme {
  | Light => light
  | Dark => dark
  }
