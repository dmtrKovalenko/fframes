module Window = {
  @val @scope("window") external devicePixelRatio: float = "devicePixelRatio"
}

module Document = {
  @send external hasFocus: Webapi.Dom.Document.t => bool = "hasFocus"
}

module Element = {
  @get external style: Webapi.Dom.Element.t => {..} = "style"
  @get external firstChild: Webapi.Dom.Element.t => option<Webapi.Dom.Element.t> = "children[0]"

  let targetAsElement = %raw(`_ => _`)

  let isFocusable = el =>
    switch el->Webapi.Dom.Element.tagName {
    | "TEXTAREA" | "SELECT" | "INPUT" | "BUTTON" | "A" => true
    | _ =>
      switch el |> Webapi.Dom.Element.getAttribute("role") {
      | Some("slider") | Some("input") | Some("button") | Some("checkbox") | Some("link") => true
      | _ => false
      }
    }
}

module Int16Array = {
  @get external length: Js.Int16Array.t => float = "length"
  @get_index external get: (Js.Int16Array.t, int) => float = ""
}
