use core::fmt;

use nom::error::{ContextError, Error, ErrorKind, ParseError};
use nom_locate::LocatedSpan;

#[derive(Debug, Clone)]
pub struct VttError {
    /// What we are looking for
    pub looking_for: String,
    /// Span with error details
    pub line: u32,
    pub offset: usize,
    pub fragment: String,
    /// Context-specific message
    pub message: Option<String>,
}

impl std::fmt::Display for VttError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            looking_for,
            line,
            offset,
            fragment,
            message,
        } = self;
        let message = match message.as_ref() {
            Some(message) => format!("\n{message}"),
            None => "".to_owned(),
        };

        write!(
            formatter,
            ".vtt parsing error at {line}:{offset}, looking for {looking_for}. Found: {fragment}{message}",
        )
    }
}

impl<'a> ParseError<LocatedSpan<&'a str>> for VttError {
    fn from_error_kind(input: LocatedSpan<&'a str>, kind: ErrorKind) -> Self {
        VttError {
            message: None,
            line: input.location_line(),
            offset: input.location_offset(),
            looking_for: format!("{kind:?}"),
            fragment: input.fragment().to_string(),
        }
    }

    fn append(input: LocatedSpan<&'a str>, kind: ErrorKind, _other: Self) -> Self {
        VttError {
            message: None,
            line: input.location_line(),
            offset: input.location_offset(),
            looking_for: format!("{kind:?}"),
            fragment: input.fragment().to_string(),
        }
    }

    fn from_char(input: LocatedSpan<&'a str>, c: char) -> Self {
        VttError {
            message: None,
            line: input.location_line(),
            offset: input.location_offset(),
            looking_for: c.to_string(),
            fragment: input.fragment().to_string(),
        }
    }

    fn or(self, other: Self) -> Self {
        let message = format!(
            "Failure. Looking for {} or {}\n",
            self.looking_for, other.looking_for
        );

        VttError {
            line: self.line,
            offset: self.offset,
            fragment: self.fragment,
            looking_for: self.looking_for,
            message: Some(message),
        }
    }
}

impl<'a> ContextError<LocatedSpan<&'a str>> for VttError {
    fn add_context(input: LocatedSpan<&'a str>, ctx: &'static str, other: Self) -> Self {
        VttError {
            message: Some(ctx.to_string()),
            line: input.location_line(),
            offset: input.location_offset(),
            looking_for: other.looking_for,
            fragment: input.fragment().to_string(),
        }
    }
}

impl<'a> From<nom::Err<Error<LocatedSpan<&'a str>>>> for VttError {
    fn from(error: nom::Err<Error<LocatedSpan<&'a str>>>) -> Self {
        match error {
            nom::Err::Error(Error { input, code }) => VttError::from_error_kind(input, code),
            nom::Err::Failure(Error { input, code }) => VttError::from_error_kind(input, code),
            nom::Err::Incomplete(_) => VttError {
                line: 0,
                offset: 0,
                fragment: "".to_owned(),
                looking_for: "".to_owned(),
                message: Some("Incomplete data, giving up parsing.".to_owned()),
            },
        }
    }
}
