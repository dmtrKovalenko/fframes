module Array = {
  let last = arr => arr->Js.Array.get(arr->Array.length - 1)

  @send
  external spliceInPlace: (array<'a>, ~start: int, ~remove: int) => array<'a> = "splice"

  let removeInPlace = (arr, ~index) => spliceInPlace(arr, ~start=index, ~remove=1)
}

module Math = {
  @inline
  let divideFloat = (a, b) => a /. b
  @inline
  let divideInt = (a, b) => a / b
  @inline
  let divideAsFloat = (a, b) => Js.Float.fromInt(a) /. Js.Float.fromInt(b)

  @scope("Math") @val
  external floor: float => int = "floor"

  @scope("Math") @val
  external round: float => int = "round"

  @scope("Math") @val
  external maxI: (int, int) => int = "max"

  @scope("Math") @val
  external minI: (int, int) => int = "min"

  let divideWithReminder = (x, y) => {
    (Js.Math.floor(x /. y), Js.Float.mod(x, y))
  }

  let minMax = (val, ~min, ~max) => {
    val < min ? min : val > max ? max : val
  }
}

module Option = {
  let unwrap = option =>
    switch option {
    | Some(val) => val
    | None => failwith("expect option to contain value")
    }

  let flatten = option =>
    switch option {
    | Some(Some(val)) => Some(val)
    | _ => None
    }

  @inline
  let unwrapOr = (option, default) =>
    switch option {
    | Some(val) => val
    | None => default
    }

  let some = val => Some(val)

  let zip = (a, b) => {
    switch (a, b) {
    | (Some(a), Some(b)) => Some((a, b))
    | _ => None
    }
  }
}

module Log = {
  let andReturn = a => {
    Js.Console.log(a)
    a
  }
}

module Path = {
  let getFilename = path => {
    path->Js.String.replaceRegExp(%re("/^.*[\\\/]/"), "")
  }
}

module Bool = {
  let invert = a => !a
  let then = a => a ? Some() : None
}

module Duration = {
  let leftPad = n =>
    n > 9.49
      ? n->Js.Float.toFixedWithPrecision(~digits=0)
      : `0${n->Js.Float.toFixedWithPrecision(~digits=0)}`

  let formatSeconds = seconds => {
    let (hours, reminder) = Math.divideWithReminder(seconds, 3600.)
    let (minutes, seconds) = Math.divideWithReminder(reminder, 60.)

    if hours > 1.0 {
      `${hours->leftPad}:${minutes->leftPad}:${seconds->leftPad}`
    } else {
      `${minutes->leftPad}:${seconds->leftPad}`
    }
  }

  let formatFrame = (frame, fps) => {
    Math.divideAsFloat(frame, fps)->formatSeconds
  }
}

let neverRerender = React.memoCustomCompareProps(_, (_, _) => true)
