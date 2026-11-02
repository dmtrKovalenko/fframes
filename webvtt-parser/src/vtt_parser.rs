use crate::Span;
use nom::branch::alt;
use nom::character::complete::{digit1, line_ending, not_line_ending};
use nom::error::{Error, ErrorKind, context};
use nom::{IResult, Slice, bytes::complete::tag, combinator::opt, multi::many0};

use crate::cue_settings_parser::parse_cue_settings;
use crate::{START_MARKER, Time, Vtt, VttCue};

fn parse_note(input: Span<'_>) -> IResult<Span<'_>, Option<&str>> {
    let (_, line) = not_line_ending(input)?;
    let Some(suffix) = line.fragment().strip_prefix("NOTE") else {
        return Ok((input, None));
    };
    if !suffix.is_empty() && !suffix.starts_with([' ', '\t']) {
        return Ok((input, None));
    }
    // NOTE is a complete block, not a single line attached to a timing line.
    let (rest, block) = parse_payload(input)?;
    // A timing line makes this a cue whose identifier starts with NOTE. Let the
    // cue parser validate it rather than silently swallowing potentially visible text.
    if block.contains("-->") {
        return Ok((input, None));
    }
    let note = block.fragment()[4..].trim();
    Ok((rest, Some(note)))
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
    let (input, minute) = parse_time_component(input, 2, 59)?;
    let (input, _) = tag(":")(input)?;
    let (input, second) = parse_time_component(input, 2, 59)?;
    let (input, _) = tag(".")(input)?;
    let (input, millisecond) = parse_time_component(input, 3, 999)?;

    Ok((input, Time(minute * 60000 + second * 1000 + millisecond)))
}

fn parse_time_with_hours(input: Span) -> IResult<Span, Time> {
    let original = input;
    let (input, hours) = digit1(input)?;
    let hour = hours.parse::<u64>().ok().filter(|_| hours.len() >= 2);
    let Some(hour) = hour else {
        return Err(nom::Err::Failure(Error::new(original, ErrorKind::Verify)));
    };
    let (input, _) = tag(":")(input)?;
    let (input, minute) = parse_time_component(input, 2, 59)?;
    let (input, _) = tag(":")(input)?;
    let (input, second) = parse_time_component(input, 2, 59)?;
    let (input, _) = tag(".")(input)?;
    let (input, millisecond) = parse_time_component(input, 3, 999)?;

    let milliseconds = hour
        .checked_mul(3_600_000)
        .and_then(|hours| hours.checked_add(minute * 60000 + second * 1000 + millisecond))
        .ok_or_else(|| nom::Err::Failure(Error::new(original, ErrorKind::Verify)))?;
    Ok((input, Time(milliseconds)))
}

fn parse_time_component(input: Span, digits: usize, maximum: u64) -> IResult<Span, u64> {
    let original = input;
    let (input, number) = digit1(input)?;
    let value = number.parse::<u64>().ok();
    match value {
        Some(value) if number.len() == digits && value <= maximum => Ok((input, value)),
        _ => Err(nom::Err::Error(Error::new(original, ErrorKind::Verify))),
    }
}

fn parse_cue_identifier(input: Span<'_>) -> IResult<Span<'_>, Option<&str>> {
    let (rest, line) = not_line_ending(input)?;

    if line.contains("-->") || line.is_empty() {
        // if this is not an identifier return same input
        Ok((input, None))
    } else {
        let (rest, _) = line_ending(rest)?;
        Ok((rest, Some(&line)))
    }
}

fn parse_cue(input: Span) -> IResult<Span, VttCue> {
    let (input, _) = many0(line_ending)(input)?;

    let mut parse_time = alt((parse_time_without_hours, parse_time_with_hours));

    let (input, name) = parse_cue_identifier(input)?;
    let (input, start) = parse_time(input)?;
    let (input, _) = tag(" --> ")(input)?;
    let (input, end) = parse_time(input)?;
    if end.as_milliseconds() <= start.as_milliseconds() {
        return Err(nom::Err::Failure(Error::new(input, ErrorKind::Verify)));
    }
    let (input, cue_settings) = opt(parse_cue_settings)(input)?;
    let (input, _) = opt(line_ending)(input)?;

    let (input, text) = parse_payload(input)?;

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
            note: None,
            text: &text,
        },
    ))
}

/// Borrow the entire payload, preserving internal line endings and markup. Only the line
/// ending immediately before a blank separator (or EOF) is excluded from the cue text.
fn parse_payload(input: Span<'_>) -> IResult<Span<'_>, Span<'_>> {
    let mut remaining = input;
    loop {
        let (after_line, line) = not_line_ending(remaining)?;
        let length = after_line.location_offset() - input.location_offset();
        if after_line.is_empty() || line.is_empty() {
            return Ok((input.slice(length..), input.slice(..length)));
        }
        let (after_ending, _) = line_ending(after_line)?;
        if after_ending.is_empty()
            || after_ending.starts_with('\n')
            || after_ending.starts_with("\r\n")
        {
            return Ok((input.slice(length..), input.slice(..length)));
        }
        remaining = after_ending;
    }
}

pub fn parse(text: Span) -> IResult<Span, Vtt> {
    let (text, _) = context("WebVTT file must start with WEBVTT", tag(START_MARKER))(text)?;
    let (text, header_comment) = not_line_ending(text)?;
    if !header_comment.is_empty()
        && !header_comment.starts_with(' ')
        && !header_comment.starts_with('\t')
    {
        return Err(nom::Err::Failure(Error::new(text, ErrorKind::Tag)));
    }
    let (mut text, _) = opt(line_ending)(text)?;
    let mut slugs = std::collections::HashMap::new();
    while !text.is_empty() {
        let (rest, line) = not_line_ending(text)?;
        let (rest, _) = opt(line_ending)(rest)?;
        text = rest;
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.fragment().split_once(':') {
            slugs.insert(key, value.trim_start());
        }
    }

    let mut cues = Vec::new();
    let mut note = None;
    loop {
        let (rest, _) = many0(line_ending)(text)?;
        text = rest;
        if text.is_empty() {
            break;
        }
        let (rest, next_note) = parse_note(text)?;
        if next_note.is_some() {
            note = next_note;
            text = rest;
            continue;
        }
        let (rest, mut cue) = parse_cue(text)?;
        cue.note = note.take();
        cues.push(cue);
        text = rest;
    }

    Ok((
        text,
        Vtt {
            cues,
            style: None,
            slugs,
        },
    ))
}
