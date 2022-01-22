use nom::error::{ContextError, Error, ErrorKind, ParseError};
use nom_locate::LocatedSpan;

pub struct WebVttError<'a> {
    /// What we are looking for
    pub looking_for: String,
    /// Span with error details
    pub input: LocatedSpan<&'a str>,
    /// Context-specific message
    pub message: Option<String>,
}

impl<'a> ParseError<LocatedSpan<&'a str>> for WebVttError<'a> {
    fn from_error_kind(input: LocatedSpan<&'a str>, kind: ErrorKind) -> Self {
        WebVttError {
            message: None,
            looking_for: format!("{:?}", kind),
            input: input.to_owned(),
        }
    }

    fn append(input: LocatedSpan<&'a str>, kind: ErrorKind, _other: Self) -> Self {
        WebVttError {
            message: None,
            looking_for: format!("{:?}", kind),
            input: input.to_owned(),
        }
    }

    fn from_char(input: LocatedSpan<&'a str>, c: char) -> Self {
        WebVttError {
            message: None,
            looking_for: c.to_string(),
            input: input.to_owned(),
        }
    }

    fn or(self, other: Self) -> Self {
        let message = format!(
            "Failure. Looking for {} or {}\n",
            self.looking_for, other.looking_for
        );

        WebVttError {
            input: self.input.to_owned(),
            looking_for: self.looking_for,
            message: Some(message),
        }
    }
}

impl<'a> ContextError<LocatedSpan<&'a str>> for WebVttError<'a> {
    fn add_context(input: LocatedSpan<&'a str>, ctx: &'static str, other: Self) -> Self {
        WebVttError {
            message: Some(ctx.to_string()),
            input: input.to_owned(),
            looking_for: other.looking_for,
        }
    }
}

impl<'a> From<nom::Err<Error<LocatedSpan<&'a str>>>> for WebVttError<'a> {
    fn from(error: nom::Err<Error<LocatedSpan<&'a str>>>) -> Self {
        match error {
            nom::Err::Error(Error { input, code }) => WebVttError::from_error_kind(input, code),
            nom::Err::Failure(Error { input, code }) => WebVttError::from_error_kind(input, code),
            nom::Err::Incomplete(_) => WebVttError {
                input: LocatedSpan::from(""),
                looking_for: "".to_owned(),
                message: Some("Incomplete data, giving up parsing.".to_owned()),
            },
        }
    }
}
