use crate::Span;
use nom::branch::alt;
use nom::character::complete::{digit1, newline, space0};
use nom::error::{context, Error};
use nom::{
    bytes::complete::{is_not, tag, take, take_until},
    combinator::opt,
    multi::{many0, many1},
    sequence::{preceded, separated_pair},
    IResult,
};

use crate::cue_settings_parser::parse_cue_settings;
use crate::{Time, Vtt, VttCue, START_MARKER};

fn parse_note(input: Span) -> IResult<Span, Option<String>> {
    let (rest, line) = take_until("\n")(input)?;

    if !line.contains("NOTE ") {
        // if NOTE not found return same input
        return Ok((input, None));
    }

    // note body is the whole line before \n that we get on :29 without "NOTE "
    let (note_body, _) = tag("NOTE ")(line)?;
    let (rest, _) = newline(rest)?;

    Ok((rest, Some(note_body.to_string())))
}

pub(crate) fn parse_number<TNumber: std::str::FromStr>(input: Span) -> IResult<Span, TNumber> {
    let (input, digit) = digit1(input)?;

    digit
        .parse::<TNumber>()
        .map(|val| (input, val))
        .map_err(|_| {
            nom::Err::Failure(nom::error::Error {
                input,
                code: nom::error::ErrorKind::Digit,
            })
        })
}

fn parse_time_without_hours(input: Span) -> IResult<Span, Time> {
    let (input, minute) = parse_number::<u64>(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, second) = parse_number::<u64>(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, millisecond) = parse_number::<u64>(input)?;

    Ok((input, Time(minute * 60000 + second * 1000 + millisecond)))
}

fn parse_time_with_hours(input: Span) -> IResult<Span, Time> {
    let (input, hour) = parse_number::<u64>(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, minute) = parse_number::<u64>(input)?;
    let (input, _) = tag(":")(input)?;
    let (input, second) = parse_number::<u64>(input)?;
    let (input, _) = tag(".")(input)?;
    let (input, millisecond) = parse_number::<u64>(input)?;

    Ok((
        input,
        Time(hour * 3600000 * minute * 60000 + second * 1000 + millisecond),
    ))
}

fn parse_cue_identifier(input: Span) -> IResult<Span, Option<String>> {
    let (rest, line) = take_until("\n")(input)?;

    if line.contains("-->") {
        // if this is not an identifier return same input
        Ok((input, None))
    } else {
        let (rest, _) = newline(rest)?;
        Ok((rest, Some(line.to_string())))
    }
}

fn parse_cue(input: Span) -> IResult<Span, VttCue> {
    let (input, _) = opt(newline)(input)?;
    let (input, _) = opt(newline)(input)?;

    let mut parse_time = alt((parse_time_without_hours, parse_time_with_hours));

    let (input, note) = parse_note(input)?;
    let (input, name) = parse_cue_identifier(input)?;
    let (input, start) = parse_time(input)?;
    let (input, _) = tag(" --> ")(input)?;
    let (input, end) = parse_time(input)?;
    let (input, cue_settings) = opt(parse_cue_settings)(input)?;
    let (input, _) = opt(newline)(input)?;

    let (input, text) = match take_until::<_, _, Error<_>>("\n")(input) {
        Ok(val) => val,
        Err(_) => take(input.len())(input)?,
    };

    Ok((
        input,
        VttCue {
            start,
            end,
            name,
            cue_settings: match cue_settings {
                Some(settings) if settings.is_empty() => None,
                other => other,
            },
            note: note.map(String::from),
            text: text.to_string(),
        },
    ))
}

pub fn parse(text: Span) -> IResult<Span, Vtt> {
    let (text, _) = context("WebVTT file must start with WEBVTT", tag(START_MARKER))(text)?;
    let (subtitles_part, slug_part) = take_until("\n\n")(text)?;

    let (text, slugs) = many0(separated_pair(
        preceded(newline, take_until(":")),
        tag(":"),
        preceded(space0, is_not("\n")),
    ))(slug_part)?;

    let (rest, cues) = many1(parse_cue)(subtitles_part)?;
    let (rest, _) = many0(newline)(rest)?;

    if !rest.is_empty() {
        let _ = parse_cue(rest)?;
    }

    Ok((
        text,
        Vtt {
            cues,
            style: None,
            slugs: slugs
                .into_iter()
                .map(|(key, val)| (key.to_string(), val.to_string()))
                .collect::<std::collections::HashMap<_, _>>(),
        },
    ))
}
