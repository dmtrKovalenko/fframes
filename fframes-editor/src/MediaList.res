open Cx
open Belt

type listVariant = Grid | List

module LoadedMediaIcon = {
  @react.component
  let make = (~variant, ~name, ~media: MediaLoader.processedMedia) => {
    let containerClassName = cx([
      "flex shrink-0 items-center justify-center overflow-hidden bg-primary-soft-alpha text-secondary",
      switch variant {
      | Grid => "aspect-square w-full rounded-xl [&>svg]:size-8"
      | List => "size-10 rounded-lg [&>svg]:size-5"
      },
    ])

    <div className=containerClassName>
      {switch media {
      | Image({src}) =>
        React.cloneElement(
          <img src alt=name className="size-full object-cover" />,
          {"loading": "lazy"},
        )
      | Audio(_) => <Icons.MusicIcon />
      | Font(_) => <Icons.FontIcon />
      | Subtitles(_) => <Icons.CaptionsIcon />
      }}
    </div>
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
  | _ => "Unknown"
  }
}

module LoadedMedia = {
  @react.component
  let make = (~name, ~media: MediaLoader.processedMedia, ~variant) => {
    <li
      title={name}
      className={switch variant {
      | Grid => "flex min-w-0 flex-col gap-2"
      | List => "flex items-center gap-3 rounded-lg px-2 py-2"
      }}>
      <LoadedMediaIcon media name variant />
      <div className="flex min-w-0 flex-1 flex-col">
        <p className="truncate text-sm text-default"> {name->React.string} </p>
        <p className="truncate text-xs text-secondary">
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

let memo = React.memoCustomCompareProps(_, (propsB, propsA) => {
  propsA["variant"] === propsB["variant"]
})

@react.component
let make = memo((~variant: listVariant) => {
  let mediaState = MediaLoader.MediaLoaderObserver.useObservable()

  <ul
    ariaLabel="Media"
    className={switch variant {
    | Grid => "grid grid-cols-[repeat(auto-fill,minmax(7rem,1fr))] gap-x-3 gap-y-4 px-4"
    | List => "flex flex-col px-2"
    }}>
    {mediaState.mediaList
    ->Map.String.keysToArray
    ->Array.map(name => {
      let media = mediaState.mediaList->Map.String.getExn(name)

      {
        switch media {
        | Media(media) => <LoadedMedia key={name} variant name media />
        // Might reconsider this choice but adding rendering ton of spinners does look wordse
        | Loading(_) => React.null
        | _ => React.null
        }
      }
    })
    ->React.array}
  </ul>
})
