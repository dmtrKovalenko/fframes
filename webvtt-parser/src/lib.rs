mod cue_settings_parser;
pub mod error;
mod vtt_parser;
pub use error::VttError;
use nom_locate::LocatedSpan;
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Formatter};

// The magic number at the start of each file
const START_MARKER: &str = "WEBVTT";

/// A start/end time of
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Time(pub(crate) u64);

impl Time {
    #[inline]
    pub fn as_milliseconds(&self) -> u64 {
        self.0
    }

    #[inline]
    pub fn from_milliseconds(millis: u64) -> Self {
        Self(millis)
    }
}

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
        let (hours, reminder) = div_rem(self.0, 3_600_000);
        let (minutes, reminder) = div_rem(reminder, 60_000);
        let (seconds, milliseconds) = div_rem(reminder, 1000);

        if hours > 0 {
            write!(
                formatter,
                "{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}",
            )
        } else {
            write!(formatter, "{minutes:02}:{seconds:02}.{milliseconds:03}",)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
                NumberOrPercentage::Percentage(percentage) => format!("{percentage}%"),
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VttCueSettings {
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

impl VttCueSettings {
    pub(crate) fn is_empty(&self) -> bool {
        self.size.is_none()
            && self.position.is_none()
            && self.vertical.is_none()
            && self.line.is_none()
            && self.align.is_none()
    }
}

impl Display for VttCueSettings {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        fn format_opt<T: Display>(name: &str, option: Option<T>) -> String {
            option
                .map(|value| format!(" {name}:{value}"))
                .unwrap_or_else(|| "".to_owned())
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VttCue<'a> {
    pub start: Time,
    pub end: Time,
    /// The identifier is a name that identifies the cue. It can be used to reference the cue from a script. It must not contain a newline and cannot contain the string "-->". It must end with a single newline.
    ///
    /// Ref: https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API#cue_identifier
    pub name: Option<&'a str>,
    pub text: &'a str,
    pub note: Option<&'a str>,
    /// Optional cue settings that belongs to this particular group. If value is Some(CueSettings) it means that at least one settings passed.
    ///
    /// Ref: https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API#cue_settings
    pub cue_settings: Option<VttCueSettings>,
}

impl Display for VttCue<'_> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}{}{} --> {}{}\n{}\n",
            self.note
                .as_ref()
                .map(|comment| format!("NOTE {comment}\n"))
                .unwrap_or_else(|| "".to_owned()),
            self.name
               .as_ref()
                .map(|comment| format!("NOTE {comment}\n"))
                .unwrap_or_else(|| "".to_owned()),
            self.start,
            self.end,
            self.cue_settings
                .as_ref()
                .map(|setting| format!("{setting}"))
                .unwrap_or_else(|| "".to_owned()),
            self.text
        )
    }
}

/// (web)VTT — Web Video Text Tracks
/// This struct represents a parsed VTT file. It contains a list of cues and optional metadata.
///
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Vtt<'a> {
    pub slugs: HashMap<&'a str, &'a str>,
    pub style: Option<&'a str>,
    pub cues: Vec<VttCue<'a>>,
}

impl<'a> Vtt<'a> {
    /// Parse [webvtt subtitles](https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API) from provided string.
    /// Make sure that it does not allocate any string data and only references parts of the original string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use webvtt_parser::{Vtt, VttCue, VttCueSettings, Align, Time};
    ///
    /// let vtt = Vtt::parse("WEBVTT
    ///
    /// 00:00.000 --> 00:05.000
    /// Hey subtitle one
    ///
    /// 00:05.000 --> 00:08.000 align:end
    /// Hey subtitle two
    ///").unwrap();
    ///
    /// assert_eq!(vtt.cues.len(), 2);
    /// assert_eq!(vtt.cues[0], VttCue { start: Time::from_milliseconds(0), end: Time::from_milliseconds(5000), text: "Hey subtitle one", name: None, note: None, cue_settings: None });
    /// assert_eq!(vtt.cues[1].cue_settings, Some(VttCueSettings { align: Some(Align::End), position: None, vertical: None, size: None, line: None }));
    /// ```
    pub fn parse(content: &'a str) -> Result<Self, VttError> {
        let content = Span::from(content);

        let (_, vtt) = vtt_parser::parse(content)?;

        Ok(vtt)
    }
}

impl Display for Vtt<'_> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "{}\n\n{}",
            START_MARKER,
            self.cues
                .iter()
                .map(|subtitle| format!("{subtitle}\n"))
                .collect::<String>()
        )
    }
}

pub type Span<'a> = LocatedSpan<&'a str>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn load_and_parse_vtt_file() {
        let content = fs::read_to_string("tests/complex-vtt-example.vtt").unwrap();

        let expected_vtt = Vtt {
        slugs: [
            ("Kind", "captions"),
            ("Language", "en"),
        ]
        .iter()
        .cloned()
        .collect::<HashMap<&str, &str>>(),
        style: None,
        cues: vec![
            VttCue {
                start: Time(9000),
                end: Time(11000),
                name: None,
                text: "<v Roger Bingham>We are in New York City",
                note: None,
                cue_settings: Some(VttCueSettings {
                    vertical: Some(Vertical::RightToLeft),
                    line: None,
                    position: None,
                    size: Some(50),
                    align: Some(Align::End),
                }),
            },
            VttCue {
                start: Time(11000),
                end: Time(13000),
                name: None,
                text: "<v Roger Bingham>We are in New York City",
                note: None,
                cue_settings: Some(VttCueSettings {
                    vertical: None,
                    line: Some(NumberOrPercentage::Number(1)),
                    position: Some(100),
                    size: None,
                    align: None,
                }),
            },
            VttCue {
                start: Time(13000),
                end: Time(16000),
                name: None,
                text: "<v Roger Bingham>We're actually at the Lucern Hotel, just down the street",
                note: None,
                cue_settings: Some(VttCueSettings {
                    vertical: None,
                    line: Some(NumberOrPercentage::Percentage(0)),
                    position: None,
                    size: None,
                    align: None,
                }),
            },
            VttCue {
                start: Time(16000),
                end: Time(18000),
                name: None,
                text: "<v Roger Bingham>from the American Museum of Natural History",
                note: None,
                cue_settings: None,
            },
            VttCue {
                start: Time(18000),
                end: Time(20000),
                name: None,
                text: "— It will perforate your stomach.",
                note: None,
                cue_settings: None,
            },
            VttCue {
                start: Time(20000),
                end: Time(22000),
                name: None,
                text: "<v Roger Bingham>Astrophysicist, Director of the Hayden Planetarium",
                note: None,
                cue_settings: None,
            },
            VttCue {
                start: Time(22000),
                end: Time(24000),
                name: None,
                text: "<v Roger Bingham>at the AMNH.",
                note: None,
                cue_settings: None,
            },
            VttCue {
                start: Time(24000),
                end: Time(26000),
                name: None,
                text: "<v Roger Bingham>Thank you for walking down here.",
                note: Some("this is comment"),
                cue_settings: None,
            },
            VttCue {
                start: Time(27000),
                end: Time(30000),
                name: Some("this is title"),
                text: "<v Roger Bingham>And I want to do a follow-up on the last conversation we did.",
                note: None,
                cue_settings: None,
            },
            VttCue {
                start: Time(30000),
                end: Time(31500),
                name: None,
                text: "<v Roger Bingham>When we e-mailed—",
                note: None,
                cue_settings: None,
            },
            VttCue {
                start: Time(30500),
                end: Time(32500),
                name: None,
                text: "<v Neil deGrasse Tyson>Didn't we talk about enough in that conversation?",
                note: None,
                cue_settings: Some(VttCueSettings {
                    vertical: None,
                    line: None,
                    position: None,
                    size: Some(50),
                    align: None,
                }),
            },
            VttCue {
                start: Time(32000),
                end: Time(35500),
                name: None,
                text: "<v Roger Bingham>No! No no no no; 'cos 'cos obviously 'cos",
                note: None,
                cue_settings: Some(VttCueSettings {
                    vertical: None,
                    line: None,
                    position: Some(30),
                    size: Some(50),
                    align: Some(Align::End),
                }),
            },
            VttCue {
                start: Time(32500),
                end: Time(33500),
                name: None,
                text: "<v Neil deGrasse Tyson><i>Laughs</i>",
                note: None,
                cue_settings: Some(VttCueSettings {
                    vertical: None,
                    line: None,
                    position: None,
                    size: Some(50),
                    align: Some(Align::Start),
                }),
            },
            VttCue {
                start: Time(35500),
                end: Time(38000),
                name: None,
                text: "<v Roger Bingham>You know I'm so excited my glasses are falling off here.",
                note: None,
                cue_settings: None,
            },
        ],
    };

        assert_eq!(Vtt::parse(&content).unwrap(), expected_vtt);
    }

    #[test]
    fn incomplete_file() {
        let content = fs::read_to_string("tests/incomplete.vtt").unwrap();

        match Vtt::parse(&content) {
            Ok(_) => panic!("The data is incomplete, should fail."),
            Err(error) => {
                assert_eq!(error.looking_for, "Digit");
                assert_eq!(&error.fragment, Span::from("").fragment());
            }
        }
    }

    #[test]
    fn invalid_file() {
        match Vtt::parse(include_str!("../tests/invalid.vtt")) {
            Ok(_) => panic!("The data is invalid, should fail."),
            Err(VttError {
                looking_for,
                fragment,
                ..
            }) => {
                assert_eq!(looking_for, "Tag");
                assert_eq!(
                    fragment,
                    Span::from(",000\nHey subtitle two\n\n")
                        .fragment()
                        .to_owned()
                );
            }
        }
    }

    #[test]
    fn simple_output() {
        let content = include_str!("../tests/simple.vtt");

        let vtt = Vtt::parse(content).unwrap();
        assert_eq!(format!("{}", vtt), content)
    }

    #[test]
    fn no_newline() {
        match Vtt::parse(include_str!("../tests/no_newline.vtt")) {
            Ok(_) => (),
            Err(VttError { .. }) => panic!("The data is valid, shouldn't fail."),
        }
    }

    #[test]
    fn with_optional_hours_in_timestamps() {
        let content = include_str!("../tests/hours.vtt");

        assert_eq!(
            Vtt::parse(content).unwrap(),
            Vtt {
                slugs: HashMap::new(),
                style: None,
                cues: vec![
                    VttCue {
                        start: Time(0),
                        end: Time(2560),
                        name: None,
                        text: " Some people literally cannot go to the doctor.",
                        note: None,
                        cue_settings: None,
                    },
                    VttCue {
                        start: Time(2560),
                        end: Time(5040),
                        name: None,
                        text: " If they get sick, they just hope that they get better",
                        note: None,
                        cue_settings: None,
                    },
                ],
            }
        );
    }
}
