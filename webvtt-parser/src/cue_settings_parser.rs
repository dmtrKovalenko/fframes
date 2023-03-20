use crate::Span;
use nom::character::complete::space0;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    combinator::opt,
    sequence::{separated_pair, terminated},
    IResult,
};

use crate::vtt_parser::parse_number;
use crate::{Align, NumberOrPercentage, Vertical, VttCueSettings};

fn parse_percentage(input: Span) -> IResult<Span, u8> {
    terminated(parse_number::<u8>, tag("%"))(input)
}

fn parse_percentage_as_generic(input: Span) -> IResult<Span, NumberOrPercentage> {
    let (input, percentage) = parse_percentage(input)?;
    Ok((input, NumberOrPercentage::Percentage(percentage)))
}

fn parse_number_as_generic(input: Span) -> IResult<Span, NumberOrPercentage> {
    let (input, number) = parse_number::<i32>(input)?;
    Ok((input, NumberOrPercentage::Number(number)))
}

fn parse_line(input: Span) -> IResult<Span, NumberOrPercentage> {
    let (input, _) = take_until("line:")(input)?;
    let (input, (_, value)) = separated_pair(
        tag("line"),
        tag(":"),
        alt((parse_percentage_as_generic, parse_number_as_generic)),
    )(input)?;

    Ok((input, value))
}

fn parse_align(input: Span) -> IResult<Span, Align> {
    let (input, _) = take_until("align:")(input)?;
    let (input, (_, align)) = separated_pair(
        tag("align"),
        tag(":"),
        alt((tag("end"), tag("start"), tag("middle"))),
    )(input)?;

    match *align {
        "end" => Ok((input, Align::End)),
        "start" => Ok((input, Align::Start)),
        "middle" => Ok((input, Align::Middle)),
        _ => Err(nom::Err::Failure(nom::error::Error {
            input,
            code: nom::error::ErrorKind::Alt,
        })),
    }
}

fn parse_vertical(input: Span) -> IResult<Span, Vertical> {
    let (input, _) = take_until("vertical:")(input)?;
    let (input, (_, vertical)) =
        separated_pair(tag("vertical"), tag(":"), alt((tag("rl"), tag("lr"))))(input)?;

    Ok((
        input,
        if vertical.to_string() == *"rl" {
            Vertical::RightToLeft
        } else {
            Vertical::LeftToRight
        },
    ))
}

fn parse_position(input: Span) -> IResult<Span, u8> {
    let (input, _) = take_until("position:")(input)?;
    let (input, (_, value)) = separated_pair(tag("position"), tag(":"), parse_percentage)(input)?;

    Ok((input, value))
}

fn parse_size(input: Span) -> IResult<Span, u8> {
    let (input, _) = take_until("size:")(input)?;
    let (input, (_, value)) = separated_pair(tag("size"), tag(":"), parse_percentage)(input)?;

    Ok((input, value))
}

pub(crate) fn parse_cue_settings(input: Span) -> IResult<Span, VttCueSettings> {
    let (input, _) = space0(input)?;
    let (input, cue_settings_part) = take_until("\n")(input)?;

    let (_, size) = opt(parse_size)(cue_settings_part)?;
    let (_, position) = opt(parse_position)(cue_settings_part)?;
    let (_, vertical) = opt(parse_vertical)(cue_settings_part)?;
    let (_, line) = opt(parse_line)(cue_settings_part)?;
    let (_, align) = opt(parse_align)(cue_settings_part)?;

    Ok((
        input,
        VttCueSettings {
            vertical,
            align,
            line,
            position,
            size,
        },
    ))
}
