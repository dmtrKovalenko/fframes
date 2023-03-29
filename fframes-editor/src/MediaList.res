open Belt
open Cx

type listVariant = Grid | List

module LoadedMediaIcon = {
  let iconContainerClassName = "overflow-hidden bg-gray-400 bg-gradient-to-tr from-indigo-400 to-pink-400 flex justify-center items-center"

  @react.component
  let make = (~media: MediaLoader.processedMedia, ~variant) => {
    let style = switch variant {
    | Grid => ReactDOM.Style.make(~width="7.4rem", ~height="7.4rem", ~borderRadius="1.35rem", ())
    | List => ReactDOM.Style.make(~width="2.5rem", ~height="2.5rem", ~borderRadius="0.75rem", ())
    }

    let iconClassName = switch variant {
    | Grid => "h-[40%]"
    | List => "h-[60%]"
    }

    switch media {
    | Image({src}) =>
      <div
        className={cx(["bg-cover bg-no-repeat bg-center", iconContainerClassName])}
        style={ReactDOMStyle.make(~backgroundImage=`url(${src})`, ())->ReactDOM.Style.combine(
          style,
        )}
      />

    | nonImageMedia =>
      <div className=iconContainerClassName style>
        {switch nonImageMedia {
        | Audio(_) => <Icons.MusicalNotesIcon color="currentColor" className=iconClassName />
        | Font(_) => <Icons.FontIcon color="currentColor" className=iconClassName />
        | Subtitles(_) => <Icons.CaptionsIcon color="currentColor" className=iconClassName />
        | _ => React.null
        }}
      </div>
    }
  }
}

let stringifyFontWeight = weight => {
  switch weight {
  | 100 => "Thin"
  | 200 => "Extra light"
  | 300 => "Light"
  | 400 => "Regular"
  | 500 => "Medium"
  | 600 => "Semi Bold"
  | 700 => "Bold"
  | 800 => "Extra bold"
  | 900 => "Black"
  | _ => ""
  }
}

module LoadedMedia = {
  @react.component
  let make = (~name, ~media: MediaLoader.processedMedia, ~variant) => {
    <li
      title={name}
      className={Cx.cx([
        switch variant {
        | Grid => "w-32 flex flex-col space-y-2"
        | List => "py-2 h-16 2xl:h-20 flex space-x-2 px-6"
        },
      ])}>
      <LoadedMediaIcon media variant />
      <div className="ml-0.5 flex flex-col">
        <p
          className={Cx.cx([
            "text-gray-300 2xl:text-lg",
            switch variant {
            | Grid => "truncate"
            | List => "line-clamp-3"
            },
          ])}>
          {name->React.string}
        </p>
        <p className="text-gray-500 text-xs 2xl:text-base truncate">
          {switch media {
          | Audio({sampleRate, duration}) =>
            `${duration->Utils.Duration.formatSeconds}, ${sampleRate->Int.toString}hz`->React.string
          | Font(fontInfo) if fontInfo.style === "normal" =>
            `${fontInfo.name} (${fontInfo.weight->stringifyFontWeight}, ${fontInfo.unicodeRange})`->React.string
          | Font(fontInfo) =>
            switch variant {
            | Grid => fontInfo.name
            | List =>
              `${fontInfo.name} (${fontInfo.style}, ${fontInfo.weight->stringifyFontWeight}, ${fontInfo.unicodeRange})`
            }->React.string
          | Subtitles(phrasesCount) => React.string(`${phrasesCount->Int.toString} phrases`)
          | Image({width, height}) => `${width->Int.toString}x${height->Int.toString}`->React.string
          }}
        </p>
      </div>
    </li>
  }
}

module Loading = {
  @react.component
  let make = (~name) => {
    <div> <p> {name->React.string} </p> </div>
  }
}

let memo = React.memoCustomCompareProps(_, (propsA, propsB) => {
  propsA["variant"] === propsB["variant"]
})

@react.component
let make = memo((~variant: listVariant) => {
  let mediaState = MediaLoader.MediaLoaderObserver.useObservable()

  <ul
    className={Cx.cx([
      switch variant {
      | Grid => "flex flex-wrap gap-x-6 gap-y-4"
      | List => "divide-y divide-gray-800 -mx-6"
      },
    ])}>
    {mediaState.mediaList
    ->Map.String.keysToArray
    ->Array.map(name => {
      let media = mediaState.mediaList->Map.String.getExn(name)

      {
        switch media {
        | Media(media) => <LoadedMedia key={name} variant name media />
        | Loading(_) => <Loading key={name} name />
        | _ => React.null
        }
      }
    })
    ->React.array}
  </ul>
})
