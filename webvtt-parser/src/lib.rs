mod cue_settings_parser;
pub mod error;
mod vtt_parser;
extern crate nom;
use error::WebVttError;
use nom_locate::LocatedSpan;
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Formatter};

// The magic number at the start of each file
const START_MARKER: &str = "WEBVTT";

/// A start/end time of
#[derive(Debug, PartialEq, Clone)]
pub struct Time(pub u64);

pub fn div_rem<T: std::ops::Div<Output = T> + std::ops::Rem<Output = T> + Copy>(
    x: T,
    y: T,
) -> (T, T) {
    let quot = x / y;
    let rem = x % y;
    (quot, rem)
}

impl Display for Time {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        // print hour if needed
        let (hours, reminder) = div_rem(self.0, 3600_000);
        let (minutes, reminder) = div_rem(reminder, 60_000);
        let (seconds, milliseconds) = div_rem(reminder, 1000);

        if hours > 0 {
            write!(
                formatter,
                "{:02}:{:02}:{:02}.{:03}",
                hours, minutes, seconds, milliseconds
            )
        } else {
            write!(
                formatter,
                "{:02}:{:02}.{:03}",
                minutes, seconds, milliseconds
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Vertical {
    RightToLeft,
    LeftToRight,
}

impl Display for Vertical {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}",
            match self {
                Vertical::RightToLeft => "vertical:rt",
                Vertical::LeftToRight => "vertical:lr",
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberOrPercentage {
    Number(i32),
    Percentage(u8),
}

impl Display for NumberOrPercentage {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}",
            match self {
                NumberOrPercentage::Number(number) => number.to_string(),
                NumberOrPercentage::Percentage(percentage) => format!("{}%", percentage),
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    Start,
    Middle,
    End,
}

impl Display for Align {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}",
            match self {
                Align::Start => "start",
                Align::End => "end",
                Align::Middle => "middle",
            }
        )
    }
}

/// Cue settings are optional components used to position where the cue payload text will be displayed over the video.
/// This includes whether the text is displayed horizontally or vertically.
/// There can be zero or more of them, and they can be used in any order so long as each setting is used no more than once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CueSettings {
    pub vertical: Option<Vertical>,
    /// Specifies where text appears vertically. If vertical is set, line specifies where text appears horizontally.
    /// Value can be a line number:
    /// - The line height is the height of the first line of the cue as it appears on the video.
    /// - Positive numbers indicate top down.
    /// - Negative numbers indicate bottom up.
    ///
    /// Or value can be a percentage:
    /// - Must be an integer (i.e., no decimals) between 0 and 100 inclusive.
    /// - Must be followed by a percent sign (%).
    pub line: Option<NumberOrPercentage>,
    /// Specifies where the text will appear horizontally. If vertical is set, position specifies where the text will appear vertically. Value is percentage.
    pub position: Option<u8>,
    /// Specifies the width of the text area. If vertical is set, size specifies the height of the text area. Value is percentage.
    pub size: Option<u8>,
    /// Specifies the alignment of the text. Text is aligned within the space given by the size cue setting if it is set.
    pub align: Option<Align>,
}

impl CueSettings {
    pub(crate) fn is_empty(&self) -> bool {
        self.size.is_none()
            && self.position.is_none()
            && self.vertical.is_none()
            && self.line.is_none()
            && self.align.is_none()
    }
}

impl Display for CueSettings {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        fn format_opt<T: Display>(name: &str, option: Option<T>) -> String {
            option
                .map(|value| format!(" {}:{}", name, value))
                .unwrap_or("".to_owned())
        }

        write!(
            formatter,
            "{}{}{}{}{}",
            format_opt("vertical", self.vertical),
            format_opt("size", self.size),
            format_opt("position", self.position),
            format_opt("line", self.line),
            format_opt("align", self.align)
        )
    }
}

/// A subtitle and associated metadata
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub start: Time,
    pub end: Time,
    /// The identifier is a name that identifies the cue. It can be used to reference the cue from a script. It must not contain a newline and cannot contain the string "-->". It must end with a single newline.
    ///
    /// Ref: https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API#cue_identifier
    pub name: Option<String>,
    pub text: String,
    pub note: Option<String>,
    /// Optional cue settings that belongs to this particular group. If value is Some(CueSettings) it means that at least one settings passed.
    ///
    /// Ref: https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API#cue_settings
    pub cue_settings: Option<CueSettings>,
}

impl Display for Cue {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}{}{} --> {}{}\n{}\n",
            self.note
                .as_ref()
                .map(|comment| format!("NOTE {}\n", comment))
                .unwrap_or("".to_owned()),
            self.name
                .as_ref()
                .map(|comment| format!("NOTE {}\n", comment))
                .unwrap_or("".to_owned()),
            self.start,
            self.end,
            self.cue_settings
                .as_ref()
                .map(|setting| format!("{}", setting))
                .unwrap_or("".to_owned()),
            self.text
        )
    }
}

/// The subtitle file and metadata
#[derive(Debug, PartialEq)]
pub struct Vtt {
    pub slugs: HashMap<String, String>,
    pub style: Option<String>,
    pub cues: Vec<Cue>,
}

impl Display for Vtt {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}\n\n{}",
            START_MARKER,
            self.cues
                .iter()
                .map(|subtitle| format!("{}\n", subtitle))
                .collect::<String>()
        )
    }
}

impl<'a> Debug for WebVttError<'a> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "\n\nparse error: was looking for {}\nmessage: {:?}\nextra: {:?}\n\n{} | {}\n\n",
            self.looking_for,
            self.message,
            self.input.extra,
            self.input.location_line(),
            self.input.fragment(),
        )
    }
}

/// Parse [webvtt subtitles](https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API) from provided string.
/// # Example
/// ```rust
/// use webvtt_parser::{parse_vtt, Cue, CueSettings, Align, Time};
///
///
/// let vtt = parse_vtt("WEBVTT
///
/// 00:00.000 --> 00:05.000
/// Hey subtitle one
///
/// 00:05.000 --> 00:08.000 align:end
/// Hey subtitle two
///").unwrap();
///
/// assert_eq!(vtt.cues.len(), 2);
/// assert_eq!(vtt.cues[0], Cue { start: Time(0), end: Time(5000), text: "Hey subtitle one".to_owned(), name: None, note: None, cue_settings: None });
/// assert_eq!(vtt.cues[1].cue_settings, Some(CueSettings { align: Some(Align::End), position: None, vertical: None, size: None, line: None }));
/// ```

pub type Span<'a> = LocatedSpan<&'a str>;

pub fn parse_vtt(content: &str) -> Result<Vtt, WebVttError> {
    let content_with_newline = format!("{}{}", content, "\n");
    let content = match content.ends_with("\n") {
        true => content,
        false => content_with_newline.as_str(),
    };
    let content = Span::from(content);

    let (_, vtt) = vtt_parser::parse(content)?;

    Ok(vtt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn load_and_parse_vtt_file() {
        let content = fs::read_to_string("tests/complex-vtt-example.vtt").unwrap();

        let expected_vtt = Vtt {
        slugs: [
            ("Kind".into(), "captions".into()),
            ("Language".into(), "en".into()),
        ]
        .iter()
        .cloned()
        .collect::<HashMap<String, String>>(),
        style: None,
        cues: vec![
            Cue {
                start: Time(9000),
                end: Time(11000),
                name: None,
                text: String::from("<v Roger Bingham>We are in New York City"),
                note: None,
                cue_settings: Some(CueSettings {
                    vertical: Some(Vertical::RightToLeft),
                    line: None,
                    position: None,
                    size: Some(50),
                    align: Some(Align::End),
                }),
            },
            Cue {
                start: Time(11000),
                end: Time(13000),
                name: None,
                text: String::from("<v Roger Bingham>We are in New York City"),
                note: None,
                cue_settings: Some(CueSettings {
                    vertical: None,
                    line: Some(NumberOrPercentage::Number(1)),
                    position: Some(100),
                    size: None,
                    align: None,
                }),
            },
            Cue {
                start: Time(13000),
                end: Time(16000),
                name: None,
                text: String::from("<v Roger Bingham>We're actually at the Lucern Hotel, just down the street"),
                note: None,
                cue_settings: Some(CueSettings {
                    vertical: None,
                    line: Some(NumberOrPercentage::Percentage(0)),
                    position: None,
                    size: None,
                    align: None,
                }),
            },
            Cue {
                start: Time(16000),
                end: Time(18000),
                name: None,
                text: String::from("<v Roger Bingham>from the American Museum of Natural History"),
                note: None,
                cue_settings: None,
            },
            Cue {
                start: Time(18000),
                end: Time(20000),
                name: None,
                text: String::from("— It will perforate your stomach."),
                note: None,
                cue_settings: None,
            },
            Cue {
                start: Time(20000),
                end: Time(22000),
                name: None,
                text: String::from("<v Roger Bingham>Astrophysicist, Director of the Hayden Planetarium"),
                note: None,
                cue_settings: None,
            },
            Cue {
                start: Time(22000),
                end: Time(24000),
                name: None,
                text: String::from("<v Roger Bingham>at the AMNH."),
                note: None,
                cue_settings: None,
            },
            Cue {
                start: Time(24000),
                end: Time(26000),
                name: None,
                text: String::from("<v Roger Bingham>Thank you for walking down here."),
                note: Some("this is comment".to_owned()),
                cue_settings: None,
            },
            Cue {
                start: Time(27000),
                end: Time(30000),
                name: Some("this is title".to_owned()),
                text: String::from("<v Roger Bingham>And I want to do a follow-up on the last conversation we did."),
                note: None,
                cue_settings: None,
            },
            Cue {
                start: Time(30000),
                end: Time(31500),
                name: None,
                text: String::from("<v Roger Bingham>When we e-mailed—"),
                note: None,
                cue_settings: None,
            },
            Cue {
                start: Time(30500),
                end: Time(32500),
                name: None,
                text: String::from("<v Neil deGrasse Tyson>Didn't we talk about enough in that conversation?"),
                note: None,
                cue_settings: Some(CueSettings {
                    vertical: None,
                    line: None,
                    position: None,
                    size: Some(50),
                    align: None,
                }),
            },
            Cue {
                start: Time(32000),
                end: Time(35500),
                name: None,
                text: String::from("<v Roger Bingham>No! No no no no; 'cos 'cos obviously 'cos"),
                note: None,
                cue_settings: Some(CueSettings {
                    vertical: None,
                    line: None,
                    position: Some(30),
                    size: Some(50),
                    align: Some(Align::End),
                }),
            },
            Cue {
                start: Time(32500),
                end: Time(33500),
                name: None,
                text: String::from("<v Neil deGrasse Tyson><i>Laughs</i>"),
                note: None,
                cue_settings: Some(CueSettings {
                    vertical: None,
                    line: None,
                    position: None,
                    size: Some(50),
                    align: Some(Align::Start),
                }),
            },
            Cue {
                start: Time(35500),
                end: Time(38000),
                name: None,
                text: String::from("<v Roger Bingham>You know I'm so excited my glasses are falling off here."),
                note: None,
                cue_settings: None,
            },
        ],
    };

        assert_eq!(parse_vtt(&content).unwrap(), expected_vtt);
    }
    #[test]
    fn incomplete_file() {
        let content = fs::read_to_string("tests/incomplete.vtt").unwrap();

        match parse_vtt(&content) {
            Ok(_) => panic!("The data is incomplete, should fail."),
            Err(error) => {
                assert_eq!(error.looking_for, "Digit");
                assert_eq!((error.input.fragment()), Span::from("").fragment());
            }
        }
    }

    #[test]
    fn invalid_file() {
        let content = fs::read_to_string("tests/invalid.vtt").unwrap();

        match parse_vtt(&content) {
            Ok(_) => panic!("The data is invalid, should fail."),
            Err(WebVttError {
                looking_for,
                input,
                message,
            }) => {
                assert_eq!(looking_for, "Tag");
                assert_eq!(
                    input.fragment(),
                    Span::from(",000\nHey subtitle two").fragment()
                );
            }
        }
    }

    #[test]
    fn simple_output() {
        let content = fs::read_to_string("tests/simple.vtt").unwrap();

        let vtt = parse_vtt(&content).unwrap();
        assert_eq!(format!("{}", vtt), content)
    }
}
